//! The saved sets, app-wide: loaded from `namespace-sets.toml` at startup and
//! written back on every change, so an edit needs no separate save step.
//! Last save wins between windows - sets change rarely, through short dialogs.

use crate::config::namespaces::{NamespaceSetsConfig, SetError};
use gpui_kit::{App, BorrowAppContext as _, Global};
use std::path::PathBuf;

/// The sets, and where they're saved. No path in tests that don't care.
#[derive(Default)]
pub struct NamespaceSets {
    config: NamespaceSetsConfig,
    path: Option<PathBuf>,
}

impl Global for NamespaceSets {}

impl NamespaceSets {
    /// Loads the sets from `path` - writing an empty file on first run - and
    /// saves every change back there.
    pub fn init(path: PathBuf, cx: &mut App) {
        let config = crate::config::load(&path);
        cx.set_global(Self {
            config,
            path: Some(path),
        });
    }

    /// The saved sets - none before [`Self::init`].
    pub fn get(cx: &App) -> &NamespaceSetsConfig {
        static EMPTY: NamespaceSetsConfig = NamespaceSetsConfig { sets: Vec::new() };
        cx.try_global::<Self>()
            .map(|sets| &sets.config)
            .unwrap_or(&EMPTY)
    }

    /// Changes the sets with `change` and saves them, unless it refused.
    pub fn update<T>(
        cx: &mut App,
        change: impl FnOnce(&mut NamespaceSetsConfig) -> Result<T, SetError>,
    ) -> Result<T, SetError> {
        let mut config = Self::get(cx).clone();
        let done = change(&mut config)?;
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
        // Through `update_global`, so views observing the sets - each panel's
        // namespace picker, naming its set - hear the change.
        cx.update_global(|sets: &mut Self, _| {
            sets.config = config;
            if let Some(path) = &sets.path
                && let Err(error) = crate::config::save(path, &sets.config)
            {
                log::warn!(
                    "could not save namespace sets to {}: {error}",
                    path.display()
                );
            }
        });
        Ok(done)
    }

    /// Replaces the sets outright, unsaved - a test's starting point.
    #[cfg(test)]
    pub fn set_for_test(config: NamespaceSetsConfig, cx: &mut App) {
        cx.set_global(Self { config, path: None });
    }
}

#[cfg(test)]
mod tests;
