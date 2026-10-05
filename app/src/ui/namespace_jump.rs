//! Namespace quick-jump (`k9s-remaining-keybindings` 6): `alt-1` to `alt-9`
//! scope a namespaced list to the namespace at that position in its namespace
//! list, and `alt-0` to all namespaces - k9s's digits, with a modifier, since a
//! bare digit types into a list's filter and `ctrl-<n>` already selects tabs.
//!
//! One command per position, in a [`KEY_CONTEXT`] both namespaced list panels
//! (Pods and the generic list) wrap themselves in, so the palette offers them
//! and their keys work wherever such a list has focus. A position past the
//! list's end does nothing.

use crate::command::{Command, CommandRegistry};
use gpui_kit::*;

/// The context namespaced list panels wrap themselves in.
pub const KEY_CONTEXT: &str = "NamespacedList";
/// Where the jumps are bound: the list minus its text fields.
const KEYS_CONTEXT: &str = "NamespacedList && !Input";

/// Scope the focused list to the namespace at `position` (1-based) in its
/// namespace list - or, at 0, to all namespaces.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = namespaces, no_json)]
pub struct JumpToNamespace {
    pub position: usize,
}

/// The ids, titles and keys, position 0 (all namespaces) to 9.
const JUMPS: [(&str, &str, &str); 10] = [
    ("namespaces.jump_all", "Namespaces: Show All", "alt-0"),
    ("namespaces.jump_1", "Namespaces: Jump to 1st", "alt-1"),
    ("namespaces.jump_2", "Namespaces: Jump to 2nd", "alt-2"),
    ("namespaces.jump_3", "Namespaces: Jump to 3rd", "alt-3"),
    ("namespaces.jump_4", "Namespaces: Jump to 4th", "alt-4"),
    ("namespaces.jump_5", "Namespaces: Jump to 5th", "alt-5"),
    ("namespaces.jump_6", "Namespaces: Jump to 6th", "alt-6"),
    ("namespaces.jump_7", "Namespaces: Jump to 7th", "alt-7"),
    ("namespaces.jump_8", "Namespaces: Jump to 8th", "alt-8"),
    ("namespaces.jump_9", "Namespaces: Jump to 9th", "alt-9"),
];

pub fn register_commands(registry: &mut CommandRegistry) {
    for (position, (id, title, keys)) in JUMPS.into_iter().enumerate() {
        registry.register(Command {
            id,
            title,
            default_binding: keys,
            // Option-digit types a character in a text field on macOS.
            context: Some(KEYS_CONTEXT),
            action: Box::new(JumpToNamespace { position }),
            menu: None,
        });
    }
}

/// The namespaces a jump to `position` scopes a list to, given its namespace
/// list `names`: all (empty) at 0, the one at that position, or `None` - no
/// change - past the list's end.
pub fn scope_for(names: &[String], position: usize) -> Option<Vec<String>> {
    match position {
        0 => Some(Vec::new()),
        n => names.get(n - 1).map(|name| vec![name.clone()]),
    }
}

#[cfg(test)]
mod tests;
