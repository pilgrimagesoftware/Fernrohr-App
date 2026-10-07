//! The Logs panels preference (`logs-panel-instancing`): whether opening a
//! pod's logs opens a Logs panel of its own (the default) or reuses the one
//! Logs panel. Every way of opening logs has a flipped twin - `shift-l`, and
//! the palette's "(Other Panel Mode)" commands - that does the other once.
//!
//! The two settings are palette commands with no default key, and the
//! Settings window's Panels section dispatches them, so both take one path.

use crate::command::{Command, CommandRegistry};
use crate::config::{self, ui::LogsPanels, ui::UiConfig};
use gpui_kit::{App, Global, actions};
use std::path::{Path, PathBuf};

actions!(settings, [OpenLogsPerPod, ReuseOneLogsPanel]);

/// The current preference, and the file it's saved to - none in tests that
/// never call [`init`], so nothing is written.
struct Preference {
    panels: LogsPanels,
    path: Option<PathBuf>,
}

impl Global for Preference {}

/// Remembers the stored preference and the file (`ui.toml`) it came from.
pub fn init(panels: LogsPanels, path: PathBuf, cx: &mut App) {
    cx.set_global(Preference {
        panels,
        path: Some(path),
    });
}

/// The current preference: the one last set, else the default (per pod).
pub fn current(cx: &App) -> LogsPanels {
    cx.try_global::<Preference>()
        .map_or(LogsPanels::default(), |preference| preference.panels)
}

/// What the flipped way of opening logs does under the current preference,
/// for a hint or menu item beside `shift-l`.
pub fn flipped_label(cx: &App) -> &'static str {
    match current(cx).flipped() {
        LogsPanels::PerPod => "Logs in Own Panel",
        LogsPanels::Reuse => "Logs in Shared Panel",
    }
}

/// Makes `panels` the preference and saves it. A no-op when it's already
/// current.
pub fn set(panels: LogsPanels, cx: &mut App) {
    if panels == current(cx) {
        return;
    }
    let path = cx
        .try_global::<Preference>()
        .and_then(|preference| preference.path.clone());
    cx.set_global(Preference {
        panels,
        path: path.clone(),
    });
    if let Some(path) = path {
        save(panels, &path);
    }
}

/// Rewrites only `logs_panels` in the preference file.
fn save(panels: LogsPanels, path: &Path) {
    let mut ui: UiConfig = config::load(path);
    ui.logs_panels = panels;
    if let Err(error) = config::save(path, &ui) {
        log::warn!(
            "failed to save the Logs panels preference to {}: {error}",
            path.display()
        );
    }
}

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    for (id, title, action) in [
        (
            "settings.logs_per_pod",
            "Open Each Pod's Logs in Its Own Panel",
            Box::new(OpenLogsPerPod) as Box<dyn gpui_kit::Action>,
        ),
        (
            "settings.logs_reuse_panel",
            "Open Every Pod's Logs in One Reused Panel",
            Box::new(ReuseOneLogsPanel),
        ),
    ] {
        registry.register(Command {
            id,
            title,
            default_binding: "",
            context: None,
            action,
            menu: None,
        });
    }
}

/// The app-wide handlers. Called once at startup, beside the other windows'.
pub(crate) fn register_handlers(cx: &mut App) {
    cx.on_action(|_: &OpenLogsPerPod, cx: &mut App| set(LogsPanels::PerPod, cx));
    cx.on_action(|_: &ReuseOneLogsPanel, cx: &mut App| set(LogsPanels::Reuse, cx));
}
