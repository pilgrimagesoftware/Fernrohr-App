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

/// Loads a typed TOML config file.
///
/// With no file, it writes resolved defaults and returns them (first run). A
/// file that can't be read is left alone. A file that can't be parsed (a typo
/// in a hand edit) is copied to `<file>.bak` before the defaults come back, so
/// the next [`save`] can't lose what was in it. Both are logged.
pub fn load<T: Default + Serialize + DeserializeOwned>(path: &Path) -> T {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let defaults = T::default();
            if let Err(error) = save(path, &defaults) {
                log::warn!("couldn't write default {}: {error}", path.display());
            }
            return defaults;
        }
        Err(error) => {
            log::warn!("couldn't read {}, using defaults: {error}", path.display());
            return T::default();
        }
    };
    let parsed = std::str::from_utf8(&bytes)
        .map_err(|error| error.to_string())
        .and_then(|text| toml::from_str(text).map_err(|error| error.to_string()));
    parsed.unwrap_or_else(|error| {
        let backup = backup_path(path);
        match fs::write(&backup, &bytes) {
            Ok(()) => log::warn!(
                "couldn't parse {}, using defaults; kept a copy at {}: {error}",
                path.display(),
                backup.display()
            ),
            Err(copy_error) => log::warn!(
                "couldn't parse {}, using defaults, nor keep a copy ({copy_error}): {error}",
                path.display()
            ),
        }
        T::default()
    })
}

/// Where [`load`] keeps a copy of a file it couldn't parse: `<file>.bak` beside it.
pub(crate) fn backup_path(path: &Path) -> std::path::PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

/// Saves `value` as TOML through [`write_atomic`], creating the directory.
pub fn save<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = toml::to_string_pretty(value).map_err(std::io::Error::other)?;
    write_atomic(path, &contents)
}

static TMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Writes `contents` to `path` atomically: a temp file in the same directory,
/// then `fs::rename`d onto `path`. A same-filesystem rename is atomic on macOS
/// and Linux, so an interruption leaves `path`'s previous content or its new
/// content, never a partial file. The directory must exist.
///
/// A symlinked `path` is written through: the rename lands on the link's
/// target, so a config file kept in a dotfiles repo stays a link. An existing
/// file keeps its permissions.
pub(crate) fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::sync::atomic::Ordering;
    let resolved;
    let path = if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        resolved = fs::canonicalize(path)?;
        resolved.as_path()
    } else {
        path
    };
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
    let written = fs::write(&tmp_path, contents).and_then(|()| match fs::metadata(path) {
        Ok(existing) => fs::set_permissions(&tmp_path, existing.permissions()),
        Err(_) => Ok(()),
    });
    let renamed = written.and_then(|()| fs::rename(&tmp_path, path));
    if renamed.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    renamed
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
        let _ = fs::remove_file(backup_path(&path));
    }

    /// Tier 1 #1: a typo in a hand-edited file used to be replaced with
    /// defaults by the next load → change → save. Its text now survives in
    /// `<file>.bak`.
    #[test]
    fn a_file_that_fails_to_parse_survives_the_next_save() {
        let path = temp_path("typo");
        fs::write(&path, "name = \"kept\"\ncount = oops\n").unwrap();

        let mut loaded: Sample = load(&path);
        loaded.count = 1;
        save(&path, &loaded).unwrap();

        assert_eq!(
            fs::read_to_string(backup_path(&path)).unwrap(),
            "name = \"kept\"\ncount = oops\n"
        );
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(backup_path(&path));
    }

    /// Tier 1 #1: only a missing file gets defaults written over it. A path
    /// that exists but can't be read as a file is left as it is.
    #[test]
    fn an_unreadable_path_is_not_overwritten() {
        let path = temp_path("unreadable");
        fs::create_dir_all(&path).unwrap();

        let loaded: Sample = load(&path);

        assert_eq!(loaded, Sample::default());
        assert!(path.is_dir());
        let _ = fs::remove_dir_all(&path);
    }

    /// Tier 1 #2: `save` replaces the file in one rename, leaving no temp
    /// file behind.
    #[test]
    fn save_replaces_the_file_whole_and_leaves_no_temp_file() {
        let dir = temp_path("atomic");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample.toml");
        fs::write(&path, "name = \"old\"\ncount = 1\n").unwrap();

        let value = Sample {
            name: "new".into(),
            count: 2,
        };
        save(&path, &value).unwrap();

        assert_eq!(load::<Sample>(&path), value);
        let names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("sample.toml")]);
        let _ = fs::remove_dir_all(&dir);
    }

    /// Tier 1 #2: saving through a symlinked config file writes its target
    /// and keeps the link.
    #[cfg(unix)]
    #[test]
    fn save_writes_through_a_symlinked_file() {
        let dir = temp_path("symlink");
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("real.toml");
        let link = dir.join("sample.toml");
        fs::write(&target, "").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let value = Sample {
            name: "linked".into(),
            count: 3,
        };
        save(&link, &value).unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(load::<Sample>(&target), value);
        let _ = fs::remove_dir_all(&dir);
    }

    /// An existing file keeps its permissions across an atomic save.
    #[cfg(unix)]
    #[test]
    fn save_keeps_an_existing_files_mode() {
        use std::os::unix::fs::PermissionsExt as _;
        let path = temp_path("mode");
        fs::write(&path, "").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

        save(&path, &Sample::default()).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o640);
        let _ = fs::remove_file(&path);
    }
}
