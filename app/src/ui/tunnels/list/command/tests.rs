// NAMED imports only (no `use super::*;`): a glob of `gpui_kit::*` next to
// `#[gpui_kit::test]` shadows the builtin `#[test]` and blows the macro-expansion budget.
use super::{TUNNELS_MANAGE_COMMAND_ID, register_commands};
use crate::command::CommandRegistry;

/// The `tunnels.manage` command is registered with a title, alongside every
/// other palette command - see `ui/nav.rs`'s sibling test.
#[test]
fn tunnels_manage_is_a_registered_command() {
    let mut registry = CommandRegistry::new();
    register_commands(&mut registry);

    let command = registry
        .get(TUNNELS_MANAGE_COMMAND_ID)
        .expect("tunnels.manage must be registered");
    assert_eq!(command.title, "Manage Tunnels…");
}
