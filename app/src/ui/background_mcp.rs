//! The background MCP preference (quiet mode, #236): whether the MCP server
//! keeps serving once the last window closes and Fernrohr runs on in the
//! menu bar, or stops then and starts again when a main window opens. It is
//! the only switch the MCP server has.
//!
//! Like the other preferences, both choices are palette commands with no
//! default key, and the checkbox in Settings > Agent Access dispatches them,
//! so every route takes one path.

use crate::command::{Command, CommandRegistry};
use crate::config::{self, ui::BackgroundMcp, ui::UiConfig};
use gpui_kit::{App, Global, actions};
use std::path::{Path, PathBuf};

actions!(settings, [KeepMcpInBackground, StopMcpWithLastWindow]);

/// The checkbox's label in Settings, and what the docs call the setting.
pub const LABEL: &str = "Keep the MCP server running in the background";

/// The current preference, and the file it's saved to - none in tests that
/// never call [`init`], so nothing is written.
struct Preference {
    value: BackgroundMcp,
    path: Option<PathBuf>,
}

impl Global for Preference {}

/// Remembers the stored preference and the file (`ui.toml`) it came from.
pub fn init(value: BackgroundMcp, path: PathBuf, cx: &mut App) {
    cx.set_global(Preference {
        value,
        path: Some(path),
    });
}

/// The current preference: the one last set, else the default (keep running).
pub fn current(cx: &App) -> BackgroundMcp {
    cx.try_global::<Preference>()
        .map_or(BackgroundMcp::default(), |preference| preference.value)
}

/// Makes `value` the preference and saves it. A no-op when it's already
/// current. It takes effect the next time the last window closes.
pub fn set(value: BackgroundMcp, cx: &mut App) {
    if value == current(cx) {
        return;
    }
    let path = cx
        .try_global::<Preference>()
        .and_then(|preference| preference.path.clone());
    cx.set_global(Preference {
        value,
        path: path.clone(),
    });
    if let Some(path) = path {
        save(value, &path);
    }
}

/// Rewrites only `background_mcp` in the preference file.
fn save(value: BackgroundMcp, path: &Path) {
    let mut ui: UiConfig = config::load(path);
    ui.background_mcp = value;
    if let Err(error) = config::save(path, &ui) {
        log::warn!(
            "failed to save the background MCP preference to {}: {error}",
            path.display()
        );
    }
}

pub const KEEP_COMMAND_ID: &str = "settings.mcp_keep_in_background";
pub const STOP_COMMAND_ID: &str = "settings.mcp_stop_with_last_window";

/// Registers both choices - only where quiet mode exists (macOS): elsewhere
/// the app quits with its last window, so there is no background to keep.
pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    if !crate::util::shell::QUIET_MODE {
        return;
    }
    for (id, title, action) in [
        (
            KEEP_COMMAND_ID,
            "Keep the MCP Server Running in the Background",
            Box::new(KeepMcpInBackground) as Box<dyn gpui_kit::Action>,
        ),
        (
            STOP_COMMAND_ID,
            "Stop the MCP Server When the Last Window Closes",
            Box::new(StopMcpWithLastWindow),
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

/// The app-wide handlers. Called once at startup, beside the other
/// preferences'.
pub(crate) fn register_handlers(cx: &mut App) {
    cx.on_action(|_: &KeepMcpInBackground, cx: &mut App| set(BackgroundMcp::KeepRunning, cx));
    cx.on_action(|_: &StopMcpWithLastWindow, cx: &mut App| {
        set(BackgroundMcp::StopWithLastWindow, cx)
    });
}

#[cfg(test)]
mod tests;
