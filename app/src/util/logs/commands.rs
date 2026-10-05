//! The Logs panel's own command (`k9s-remaining-keybindings` 5): toggling
//! between a container's current logs and its previous instance's - k9s's `p`.

use crate::command::{Command, CommandRegistry};
use gpui_kit::actions;

actions!(logs, [TogglePreviousLogs]);

/// The panel's key context: its command means something only while it has focus.
pub const PANEL_KEY_CONTEXT: &str = "LogsPanel";
pub(super) const PREVIOUS_KEY: &str = "p";

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: "logs.toggle_previous",
        title: "Logs: Toggle Previous Container Logs",
        default_binding: PREVIOUS_KEY,
        context: Some(PANEL_KEY_CONTEXT),
        action: Box::new(TogglePreviousLogs),
        menu: None,
    });
}
