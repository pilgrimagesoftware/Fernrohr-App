//! The library, app-wide: loaded from `exec-library.toml` at startup and
//! written back, atomically, on every change, so an edit needs no separate save
//! step (D6). Views observing the store hear each change.
//!
//! A file that can't be read or doesn't parse is left exactly as it is - a hand
//! edit with a typo in it isn't overwritten - and the library is empty and
//! read-only until it's fixed, with the file and the error kept for the
//! window's banner. Only a missing file is an empty library; nothing is
//! written until the first change.
//!
//! The library is serialized on the main thread and written on a background one, so a
//! slow disk never stalls the UI. Only the newest serialized library is ever pending, so
//! the file always ends at the latest one, and quitting writes whatever is still pending.

use crate::config::exec_library::{EntryError, ExecLibrary};
use gpui_kit::{App, AppContext as _, BorrowAppContext as _, Global};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The library, where it's saved, and why it can't be, if the file is broken.
#[derive(Default)]
pub struct ExecLibraryStore {
    library: ExecLibrary,
    path: Option<PathBuf>,
    broken: Option<String>,
    writes: Writes,
}

impl Global for ExecLibraryStore {}

/// The newest serialized library not yet written, and where to - its lock is only held to
/// swap it, so the main thread never waits on a disk write - and the lock a write holds,
/// so two writes never interleave and an older one never lands after a newer one.
#[derive(Clone, Default)]
struct Writes {
    pending: Arc<parking_lot::Mutex<Option<(PathBuf, String)>>>,
    writing: Arc<parking_lot::Mutex<()>>,
}

impl Writes {
    /// Writes the pending library, if any. Called on a background thread after each
    /// change, and on the main thread at quit.
    fn flush(&self) {
        let _writing = self.writing.lock();
        let Some((path, text)) = self.pending.lock().take() else {
            return;
        };
        if let Err(error) = write(&path, &text) {
            log::warn!(
                "could not save the exec library to {}: {error}",
                path.display()
            );
        }
    }
}

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

fn write(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::config::write_atomic(path, text)
}

/// Serializes `library` now, replacing any library still waiting to be written, and
/// writes it to `path` on a background thread.
fn save(path: &Path, library: &ExecLibrary, writes: &Writes, cx: &App) {
    let text = match toml::to_string_pretty(library) {
        Ok(text) => text,
        Err(error) => {
            log::warn!("could not serialize the exec library: {error}");
            return;
        }
    };
    *writes.pending.lock() = Some((path.to_path_buf(), text));
    let writes = writes.clone();
    cx.background_spawn(async move { writes.flush() }).detach();
}

impl ExecLibraryStore {
    /// Loads the library from `path`, and saves every change back there.
    pub fn init(path: PathBuf, cx: &mut App) {
        let (library, broken) = match load(&path) {
            Loaded::Library(library) => (library, None),
            Loaded::Broken(reason) => (ExecLibrary::default(), Some(reason)),
        };
        let writes = Writes::default();
        cx.on_app_quit({
            let writes = writes.clone();
            move |_| {
                writes.flush();
                async {}
            }
        })
        .detach();
        cx.set_global(Self {
            library,
            path: Some(path),
            broken,
            writes,
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
        cx.update_global(|store: &mut Self, cx| {
            store.library = library;
            if let Some(path) = &store.path {
                save(path, &store.library, &store.writes, cx);
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
            writes: Writes::default(),
        });
    }
}

#[cfg(test)]
mod tests;
