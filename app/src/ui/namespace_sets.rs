//! Named namespace sets (`namespace-sets`): a user's named lists of
//! namespaces, kept in `namespace-sets.toml` ([`store`]), and the commands
//! that make, edit, delete and apply them.
//!
//! Applying a set re-scopes the focused namespaced list to exactly the set's
//! namespaces, or - Apply to Context - every namespaced list in the focused
//! panel's context and that context's default, through the window's
//! `warp_context`, which Warp All to Namespace uses too. A panel keeps the
//! namespaces it was given, not a reference to the set, so editing or deleting
//! a set never moves a panel.
//!
//! The commands are global - sets aren't tied to one panel type - so each has
//! a modifier key, and those that need a namespaced panel in focus say so when
//! there isn't one. None is in the menu bar: switching and creating act on the
//! focused panel, and the menu bar holds only commands that don't depend on
//! focus.

use crate::command::{Command, CommandRegistry};
use gpui_kit::{Action, actions};

pub mod editor;
pub mod picker;
pub mod store;

actions!(
    namespace_sets,
    [
        CreateNamespaceSet,
        EditNamespaceSet,
        RemoveNamespaceSet,
        SwitchNamespaceSet,
        SwitchContextNamespaceSet,
    ]
);

/// Applies the saved set `name` to the focused namespaced list, or with
/// `context_wide` to every namespaced list in its context and to the
/// context's default. Dispatched by the picker from the panel it was opened
/// from, once the dialog has closed and that panel has focus again.
#[derive(Clone, Debug, PartialEq, Eq, Action)]
#[action(namespace = namespace_sets, no_json)]
pub struct ApplyNamespaceSet {
    pub name: String,
    pub context_wide: bool,
}

pub const CREATE_KEY: &str = "cmd-alt-n";
pub const EDIT_KEY: &str = "cmd-alt-e";
pub const SWITCH_KEY: &str = "cmd-shift-n";
pub const SWITCH_CONTEXT_KEY: &str = "cmd-alt-shift-n";

pub const CREATE_COMMAND_ID: &str = "namespace_sets.create_set";
pub const EDIT_COMMAND_ID: &str = "namespace_sets.edit_set";
pub const REMOVE_COMMAND_ID: &str = "namespace_sets.remove_set";
pub const SWITCH_COMMAND_ID: &str = "namespace_sets.switch";
pub const SWITCH_CONTEXT_COMMAND_ID: &str = "namespace_sets.switch_context";

/// What a command run without a namespaced panel in focus says.
pub const NEEDS_NAMESPACED_PANEL: &str = "Focus a namespaced list to use a namespace set.";

/// Registers the five set commands. Remove has no default key: it's rare,
/// and asks for confirmation anyway.
pub fn register_commands(registry: &mut CommandRegistry) {
    let mut register = |id, title, default_binding, action: Box<dyn Action>| {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: None,
            action,
            menu: None,
        });
    };
    register(
        SWITCH_COMMAND_ID,
        "Switch Namespace Set…",
        SWITCH_KEY,
        Box::new(SwitchNamespaceSet),
    );
    register(
        SWITCH_CONTEXT_COMMAND_ID,
        "Apply Namespace Set to Context…",
        SWITCH_CONTEXT_KEY,
        Box::new(SwitchContextNamespaceSet),
    );
    register(
        CREATE_COMMAND_ID,
        "Create Namespace Set…",
        CREATE_KEY,
        Box::new(CreateNamespaceSet),
    );
    register(
        EDIT_COMMAND_ID,
        "Edit Namespace Set…",
        EDIT_KEY,
        Box::new(EditNamespaceSet),
    );
    register(
        REMOVE_COMMAND_ID,
        "Remove Namespace Set…",
        "",
        Box::new(RemoveNamespaceSet),
    );
}
