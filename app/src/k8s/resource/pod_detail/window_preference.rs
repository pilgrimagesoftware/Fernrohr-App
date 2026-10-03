//! The Events tab's default time window (`pod-events-time-window` 2.2): what a new
//! pod detail panel starts from, stored as `ui.toml`'s `pod_events_window`. A
//! panel that changes its own window writes it here too, so the next panel and
//! the next launch start from the latest choice.

use crate::config::{self, ui::PodEventsWindow, ui::UiConfig};
use gpui_kit::*;
use std::path::{Path, PathBuf};

/// The preferred window, and the preference file it's saved to - none in tests
/// that never call [`init`], so nothing is written.
struct Preference {
    window: PodEventsWindow,
    path: Option<PathBuf>,
}

impl Global for Preference {}

/// Remembers the stored window and the file (`ui.toml`) it came from.
pub fn init(window: PodEventsWindow, path: PathBuf, cx: &mut App) {
    cx.set_global(Preference {
        window,
        path: Some(path),
    });
}

/// The window a new panel starts on: the preference, else an hour.
pub fn preferred(cx: &App) -> PodEventsWindow {
    cx.try_global::<Preference>()
        .map_or(PodEventsWindow::default(), |preference| preference.window)
}

/// Makes `window` the preference, and saves it.
pub(super) fn set_preferred(window: PodEventsWindow, cx: &mut App) {
    let path = cx
        .try_global::<Preference>()
        .and_then(|preference| preference.path.clone());
    cx.set_global(Preference {
        window,
        path: path.clone(),
    });
    if let Some(path) = path {
        save(window, &path);
    }
}

/// Rewrites only `pod_events_window` in the preference file.
fn save(window: PodEventsWindow, path: &Path) {
    let mut ui: UiConfig = config::load(path);
    ui.pod_events_window = window;
    if let Err(error) = config::save(path, &ui) {
        log::warn!(
            "failed to save the events window to {}: {error}",
            path.display()
        );
    }
}
