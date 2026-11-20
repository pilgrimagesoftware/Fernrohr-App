//! The commands' registration: the global open command in the Window menu and
//! the palette with a default key that collides with nothing, and the row
//! actions scoped to the window's key context.

use crate::command::{CommandRegistry, MenuSlot};
use crate::ui::connections::commands::{
    DISCONNECT_COMMAND_ID, END_AGENT_HOLD_COMMAND_ID, OPEN_COMMAND_ID, OPEN_IN_WINDOW_COMMAND_ID,
    WINDOW_CONTEXT,
};

fn registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    registry
}

#[test]
fn the_open_command_is_global_and_in_the_window_menu() {
    let registry = registry();
    let command = registry.get(OPEN_COMMAND_ID).expect("registered");

    assert_eq!(command.context, None, "available from anywhere");
    assert_eq!(command.menu, Some(MenuSlot::Window));
    assert!(!command.default_binding.is_empty(), "it has a default key");
    assert!(
        registry
            .for_menu(crate::command::TopMenu::Window)
            .iter()
            .any(|command| command.id == OPEN_COMMAND_ID)
    );
}

#[test]
fn the_row_actions_are_scoped_to_the_window() {
    let registry = registry();
    for id in [
        OPEN_IN_WINDOW_COMMAND_ID,
        DISCONNECT_COMMAND_ID,
        END_AGENT_HOLD_COMMAND_ID,
    ] {
        let command = registry.get(id).expect("registered");
        assert_eq!(command.context, Some(WINDOW_CONTEXT), "{id}");
        assert!(
            registry
                .available(&[WINDOW_CONTEXT])
                .iter()
                .any(|c| c.id == id)
        );
        assert!(
            !registry.available(&["Pods"]).iter().any(|c| c.id == id),
            "{id} is not offered elsewhere"
        );
    }
}

/// No key of ours collides with another command's (task 4.2: default key
/// checked against `keymap::conflicts`).
#[test]
fn no_default_key_clashes_with_another_command() {
    let registry = registry();
    for id in [
        OPEN_COMMAND_ID,
        OPEN_IN_WINDOW_COMMAND_ID,
        DISCONNECT_COMMAND_ID,
        END_AGENT_HOLD_COMMAND_ID,
    ] {
        let command = registry.get(id).expect("registered");
        let clashes = crate::keymap::conflicts(
            &registry,
            &crate::keymap::KeymapConfig::default(),
            id,
            command.default_binding,
        );
        assert!(!clashes.any_clash(), "{id}: {clashes:?}");
        assert!(clashes.shadows.is_empty(), "{id}: {clashes:?}");
    }
}
