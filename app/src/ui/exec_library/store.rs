//! The library, app-wide: loaded from `exec-library.toml` at startup and
//! written back, atomically, on every change, so an edit needs no separate save
//! step (D6). Views observing the store hear each change.
//!
//! A file that can't be read or doesn't parse is left exactly as it is - a hand
//! edit with a typo in it isn't overwritten - and the library is empty and
//! read-only until it's fixed, with the file and the error kept for the
//! window's banner. Only a missing file is an empty library; nothing is
//! written until the first change.

use crate::config::exec_library::{EntryError, ExecLibrary};
use gpui_kit::{App, BorrowAppContext as _, Global};
use std::path::{Path, PathBuf};

/// The library, where it's saved, and why it can't be, if the file is broken.
#[derive(Default)]
pub struct ExecLibraryStore {
    library: ExecLibrary,
    path: Option<PathBuf>,
    broken: Option<String>,
}

impl Global for ExecLibraryStore {}

/// What reading the file gave.
enum Loaded {
    Library(ExecLibrary),
    /// The file is there but doesn't parse: why, naming the file.
    Broken(String),
}

fn load(path: &Path) -> Loaded {
    match std::fs::read_to_string(path) {
        Ok(text) => match toml::from_str(&text) {
            Ok(library) => Loaded::Library(library),
            Err(error) => Loaded::Broken(format!("{}: {error}", path.display())),
        },
        // Only a missing file is an empty library. Any other read error -
        // permission denied, not UTF-8, a directory - is a file that is there
        // and can't be read, so it isn't overwritten either.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Loaded::Library(ExecLibrary::default())
        }
        Err(error) => Loaded::Broken(format!("{}: {error}", path.display())),
    }
}

fn save(path: &Path, library: &ExecLibrary) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(library).expect("the library serializes to TOML");
    crate::config::write_atomic(path, &text)
}

impl ExecLibraryStore {
    /// Loads the library from `path`, and saves every change back there.
    pub fn init(path: PathBuf, cx: &mut App) {
        let (library, broken) = match load(&path) {
            Loaded::Library(library) => (library, None),
            Loaded::Broken(reason) => (ExecLibrary::default(), Some(reason)),
        };
        cx.set_global(Self {
            library,
            path: Some(path),
            broken,
        });
    }

    /// The saved entries - none before [`Self::init`], or with a broken file.
    pub fn get(cx: &App) -> &ExecLibrary {
        static EMPTY: ExecLibrary = ExecLibrary {
            entries: Vec::new(),
        };
        cx.try_global::<Self>()
            .map(|store| &store.library)
            .unwrap_or(&EMPTY)
    }

    /// Why the file couldn't be read, naming it - `None` when it's fine.
    pub fn broken(cx: &App) -> Option<&str> {
        cx.try_global::<Self>()
            .and_then(|store| store.broken.as_deref())
    }

    /// Changes the library with `change` and saves it, unless it refused or
    /// the file is broken.
    pub fn update<T>(
        cx: &mut App,
        change: impl FnOnce(&mut ExecLibrary) -> Result<T, EntryError>,
    ) -> Result<T, EntryError> {
        if Self::broken(cx).is_some() {
            return Err(EntryError::ReadOnly);
        }
        let mut library = Self::get(cx).clone();
        let done = change(&mut library)?;
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
        cx.update_global(|store: &mut Self, _| {
            store.library = library;
            if let Some(path) = &store.path
                && let Err(error) = save(path, &store.library)
            {
                log::warn!(
                    "could not save the exec library to {}: {error}",
                    path.display()
                );
            }
        });
        Ok(done)
    }

    /// Replaces the library outright, unsaved - a test's starting point.
    #[cfg(test)]
    pub fn set_for_test(library: ExecLibrary, cx: &mut App) {
        cx.set_global(Self {
            library,
            path: None,
            broken: None,
        });
    }
}

#[cfg(test)]
mod tests;
