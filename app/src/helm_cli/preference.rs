//! The Settings path for `helm` (`helm-releases` 7.1), saved as `ui.toml`'s
//! `helm_path`: unset means look `helm` up on the login shell's PATH. Setting
//! it checks the binary again ([`super::locate::refresh`]).

use crate::config::{self, ui::UiConfig};
use gpui_kit::{App, Global};
use std::path::{Path, PathBuf};

/// The current path, and the file it's saved to - none in tests that never
/// call [`init`], so nothing is written.
struct Preference {
    helm_path: Option<String>,
    file: Option<PathBuf>,
}

impl Global for Preference {}

/// Remembers the stored path and the file (`ui.toml`) it came from, and
/// starts the first check for `helm`.
pub(crate) fn init(helm_path: Option<String>, file: PathBuf, cx: &mut App) {
    let configured = configured(helm_path.as_deref());
    cx.set_global(Preference {
        helm_path,
        file: Some(file),
    });
    super::locate::refresh(configured, cx);
}

/// The path set in Settings, if any.
pub(crate) fn current(cx: &App) -> Option<String> {
    cx.try_global::<Preference>()
        .and_then(|preference| preference.helm_path.clone())
}

/// Makes `helm_path` the preference - blank meaning none - saves it, and
/// checks for `helm` again. A no-op when it's already current.
pub(crate) fn set_path(helm_path: &str, cx: &mut App) {
    let helm_path = Some(helm_path.trim().to_string()).filter(|path| !path.is_empty());
    if helm_path == current(cx) && cx.has_global::<Preference>() {
        return;
    }
    let file = cx
        .try_global::<Preference>()
        .and_then(|preference| preference.file.clone());
    cx.set_global(Preference {
        helm_path: helm_path.clone(),
        file: file.clone(),
    });
    if let Some(file) = file {
        save(helm_path.clone(), &file);
    }
    super::locate::refresh(configured(helm_path.as_deref()), cx);
}

/// The Settings path as a path, `~` expanded - what a person types.
fn configured(helm_path: Option<&str>) -> Option<PathBuf> {
    let helm_path = helm_path?;
    Some(match helm_path.strip_prefix("~/") {
        Some(rest) => {
            dirs::home_dir().map_or_else(|| PathBuf::from(helm_path), |home| home.join(rest))
        }
        None => PathBuf::from(helm_path),
    })
}

/// Rewrites only `helm_path` in the preference file.
fn save(helm_path: Option<String>, file: &Path) {
    let mut ui: UiConfig = config::load(file);
    ui.helm_path = helm_path;
    if let Err(error) = config::save(file, &ui) {
        log::warn!(
            "failed to save the helm path to {}: {error}",
            file.display()
        );
    }
}

#[cfg(test)]
mod tests;
