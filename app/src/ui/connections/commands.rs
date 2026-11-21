//! The Connections window's commands: opening it from anywhere (global, in the
//! Window menu and the status menu), and Open in Window, Disconnect and End
//! Agent's Hold on the selected row, scoped to the window's key context so the
//! palette offers them only while it has focus.
//!
//! The row actions' keys are single letters, safe because the window has no text
//! field. Disconnect's confirmation is the recoverable tier, so Enter confirms it.

use crate::command::{Command, CommandRegistry, MenuSlot};
use gpui_kit::actions;

actions!(
    connections,
    [
        ConnectionsOpen,
        ConnectionsOpenInWindow,
        ConnectionsDisconnect,
        ConnectionsEndAgentHold
    ]
);

pub(crate) const OPEN_COMMAND_ID: &str = "connections.open";
pub(crate) const OPEN_IN_WINDOW_COMMAND_ID: &str = "connections.open_in_window";
pub(crate) const DISCONNECT_COMMAND_ID: &str = "connections.disconnect";
pub(crate) const END_AGENT_HOLD_COMMAND_ID: &str = "connections.end_agent_hold";

/// The window's key context.
pub(crate) const WINDOW_CONTEXT: &str = "ConnectionsWindow";

pub(crate) const OPEN_DEFAULT_BINDING: &str = "cmd-shift-c";
pub(crate) const OPEN_IN_WINDOW_KEY: &str = "enter";
pub(crate) const DISCONNECT_KEY: &str = "d";
pub(crate) const END_AGENT_HOLD_KEY: &str = "e";

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: OPEN_COMMAND_ID,
        title: "Connections",
        default_binding: OPEN_DEFAULT_BINDING,
        context: None,
        action: Box::new(ConnectionsOpen),
        menu: Some(MenuSlot::Window),
    });
    registry.register(Command {
        id: OPEN_IN_WINDOW_COMMAND_ID,
        title: "Connections: Open in Window",
        default_binding: OPEN_IN_WINDOW_KEY,
        context: Some(WINDOW_CONTEXT),
        action: Box::new(ConnectionsOpenInWindow),
        menu: None,
    });
    registry.register(Command {
        id: DISCONNECT_COMMAND_ID,
        title: "Connections: Disconnect\u{2026}",
        default_binding: DISCONNECT_KEY,
        context: Some(WINDOW_CONTEXT),
        action: Box::new(ConnectionsDisconnect),
        menu: None,
    });
    registry.register(Command {
        id: END_AGENT_HOLD_COMMAND_ID,
        title: "Connections: End Agent's Hold",
        default_binding: END_AGENT_HOLD_KEY,
        context: Some(WINDOW_CONTEXT),
        action: Box::new(ConnectionsEndAgentHold),
        menu: None,
    });
}
