use serde::{Serialize, de::DeserializeOwned};
use std::fs;
use std::io::Write as _;
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
    private_writer(path, false)?.write_all(contents.as_bytes())
}

/// `path` opened for writing, truncated. A file it creates is owner-only (0600
/// on unix): config such as `tunnels.toml` names bastions and tunnel commands,
/// which other local users have no business reading. One that already exists
/// keeps the permissions it has - the user's to choose - and a symlink is
/// followed, as `fs::write` does. `create_new` refuses a path that exists.
fn private_writer(path: &Path, create_new: bool) -> std::io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true);
    if create_new {
        options.create_new(true);
    } else {
        options.create(true).truncate(true);
    }
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

static TMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Writes `contents` to `path` atomically: a temp file in the same directory,
/// then `fs::rename`d onto `path`. A same-filesystem rename is atomic on macOS
/// and Linux, so an interruption leaves `path`'s previous content or its new
/// content, never a partial file. The directory must exist. The result is
/// owner-only, as [`save`] makes a new file.
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
    private_writer(&tmp_path, true)?.write_all(contents.as_bytes())?;
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

    /// Security pass: a config file [`save`] or [`write_atomic`] creates is
    /// owner-only; one that already exists keeps the mode the user gave it.
    #[cfg(unix)]
    #[test]
    fn new_config_files_are_owner_only_and_existing_ones_keep_their_mode() {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;

        let saved = temp_path("private-save");
        save(&saved, &Sample::default()).unwrap();
        assert_eq!(mode(&saved), 0o600);

        let atomic = temp_path("private-atomic");
        write_atomic(&atomic, "a = 1\n").unwrap();
        assert_eq!(mode(&atomic), 0o600);

        let shared = temp_path("existing-mode");
        fs::write(&shared, "").unwrap();
        fs::set_permissions(&shared, fs::Permissions::from_mode(0o644)).unwrap();
        save(&shared, &Sample::default()).unwrap();
        assert_eq!(mode(&shared), 0o644);

        for path in [saved, atomic, shared] {
            let _ = fs::remove_file(path);
        }
    }
}
