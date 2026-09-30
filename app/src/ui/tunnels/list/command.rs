//! Registers the `tunnels.manage` command with the app-wide [`CommandRegistry`] - the
//! palette/menu-bar/keybinding route to `window::open_or_focus`.

use super::*;

actions!(tunnels, [TunnelsManage]);

pub const TUNNELS_MANAGE_COMMAND_ID: &str = "tunnels.manage";
pub const TUNNELS_MANAGE_DEFAULT_BINDING: &str = "cmd-shift-t";

/// The command this module contributes to the app-wide [`CommandRegistry`] - see
/// `util/shell.rs::register_commands` for the sibling pattern this follows.
pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: TUNNELS_MANAGE_COMMAND_ID,
        title: "Manage Tunnels…",
        default_binding: TUNNELS_MANAGE_DEFAULT_BINDING,
        context: None,
        action: Box::new(TunnelsManage),
        // The Context menu: tunnels are how a context is reached, and the menu bar
        // makes the Tunnels window reachable with no cluster window open.
        menu: Some(crate::command::MenuSlot::Context),
    });
}

#[cfg(test)]
mod tests;
