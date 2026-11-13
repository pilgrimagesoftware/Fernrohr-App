//! The status bar's keyboard routes (`toolbar-layout-with-gpui-kit` 1.2-1.3): adding
//! a context to the window and disconnecting its active one - or all of its
//! contexts, or all but the active one (#359) - as registered commands
//! in the Context menu and the palette. `MainWindow` handles them by asking its
//! status bar, which owns the Add Context window and the disconnect confirmation.

use crate::command::{Command, CommandRegistry, ContextGroup, MenuSlot};
use gpui_kit::actions;

actions!(
    context,
    [
        AddContext,
        DisconnectActiveContext,
        DisconnectAllContexts,
        DisconnectOtherContexts
    ]
);

const ADD_COMMAND_ID: &str = "context.add";
const DISCONNECT_COMMAND_ID: &str = "context.disconnect";
const DISCONNECT_ALL_COMMAND_ID: &str = "context.disconnect_all";
const DISCONNECT_OTHERS_COMMAND_ID: &str = "context.disconnect_others";

/// All global, with no default key: they're occasional, and the menu and the
/// palette reach them.
pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: ADD_COMMAND_ID,
        title: "Add Context to Window…",
        default_binding: "",
        context: None,
        action: Box::new(AddContext),
        menu: Some(MenuSlot::Context(ContextGroup::Contexts)),
    });
    registry.register(Command {
        id: DISCONNECT_COMMAND_ID,
        title: "Disconnect Active Context…",
        default_binding: "",
        context: None,
        action: Box::new(DisconnectActiveContext),
        menu: Some(MenuSlot::Context(ContextGroup::Contexts)),
    });
    registry.register(Command {
        id: DISCONNECT_ALL_COMMAND_ID,
        title: "Disconnect All Contexts\u{2026}",
        default_binding: "",
        context: None,
        action: Box::new(DisconnectAllContexts),
        menu: Some(MenuSlot::Context(ContextGroup::Contexts)),
    });
    registry.register(Command {
        id: DISCONNECT_OTHERS_COMMAND_ID,
        title: "Disconnect Other Contexts\u{2026}",
        default_binding: "",
        context: None,
        action: Box::new(DisconnectOtherContexts),
        menu: Some(MenuSlot::Context(ContextGroup::Contexts)),
    });
}
