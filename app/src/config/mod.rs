use serde::{Serialize, de::DeserializeOwned};
use std::fs;
use std::path::Path;

pub mod dock_layouts;
pub mod exec_library;
pub mod namespaces;
pub mod saved_layouts;
pub mod tunnels;
pub mod ui;
pub mod window_state;
pub mod workspace;

/// Loads a typed TOML config file, writing resolved defaults on first run and
/// falling back to defaults (leaving the file untouched) if it fails to parse.
pub fn load<T: Default + Serialize + DeserializeOwned>(path: &Path) -> T {
    match fs::read_to_string(path) {
        Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
        Err(_) => {
            let defaults = T::default();
            let _ = save(path, &defaults);
            defaults
        }
    }
}

pub fn save<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = toml::to_string_pretty(value).expect("config struct must serialize to TOML");
    fs::write(path, contents)
}

static TMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Writes `contents` to `path` atomically: a temp file in the same directory,
/// then `fs::rename`d onto `path`. A same-filesystem rename is atomic on macOS
/// and Linux, so an interruption leaves `path`'s previous content or its new
/// content, never a partial file. The directory must exist.
pub(crate) fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::sync::atomic::Ordering;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config");
    let tmp_name = format!(
        ".{file_name}.tmp-{}-{}",
        std::process::id(),
        TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let tmp_path = dir.join(tmp_name);
    fs::write(&tmp_path, contents)?;
    fs::rename(&tmp_path, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    fn temp_path(name: &str) -> std::path::PathBuf {
        crate::util::test_paths::temp_path(&format!("config-{name}"))
    }

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Sample {
        name: String,
        count: u32,
    }

    #[test]
    fn first_load_with_no_file_writes_defaults() {
        let path = temp_path("create-default");
        assert!(!path.exists());

        let loaded: Sample = load(&path);

        assert_eq!(loaded, Sample::default());
        assert!(path.exists());
        let on_disk: Sample = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(on_disk, Sample::default());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = temp_path("round-trip");
        let value = Sample {
            name: "fernrohr".into(),
            count: 7,
        };

        save(&path, &value).unwrap();
        let loaded: Sample = load(&path);

        assert_eq!(loaded, value);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn parse_failure_keeps_file_and_returns_defaults() {
        let path = temp_path("corrupt");
        fs::write(&path, "not valid toml {{{").unwrap();

        let loaded: Sample = load(&path);

        assert_eq!(loaded, Sample::default());
        assert_eq!(fs::read_to_string(&path).unwrap(), "not valid toml {{{");

        let _ = fs::remove_file(&path);
    }
}
