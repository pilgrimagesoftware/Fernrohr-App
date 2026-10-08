//! `saved-panel-layouts` section 7: the seven commands design.md D4's table
//! registers collide with nothing in the full live registry
//! (`crate::util::shell::register_commands`), and the command palette offers
//! each one only with its own `KeyContext` on the focus-path stack - the four
//! picker-scoped ones while `SavedLayoutsPicker` is active,
//! `settings.show_layouts` only while `SettingsWindow` is. Mirrors
//! `k9s_commands.rs`'s and `ui::namespace_jump::tests`'s own
//! full-registry-conflict pattern rather than adding a second one.

use super::super::{KeymapConfig, conflicts};
use crate::command::CommandRegistry;

fn app_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    registry
}

/// Every command design.md D4's table adds, with a non-empty default
/// binding - `settings.show_layouts` has none, so there is no key to check
/// for a collision (`keymap::conflicts` has nothing to parse either way).
const KEYED_COMMANDS: &[(&str, &str)] = &[
    ("layouts.save", "secondary-shift-s"),
    ("layouts.manage", "secondary-shift-o"),
    ("saved_layouts.load_replace", "enter"),
    ("saved_layouts.load_add", "secondary-enter"),
    ("saved_layouts.rename_selected", "r"),
    ("saved_layouts.delete_selected", "backspace"),
];

/// 7.1: none of the six keyed commands' default bindings collides with
/// another command the full app registers - neither in the same scope nor by
/// shadowing (or being shadowed by) a global one - the same check
/// `ui::namespace_jump::tests::the_quick_jump_keys_collide_with_nothing` runs
/// for its own new commands.
#[test]
fn the_new_commands_collide_with_nothing_in_the_full_registry() {
    let registry = app_registry();
    for (id, keys) in KEYED_COMMANDS {
        assert!(registry.get(id).is_some(), "{id} is registered app-wide");
        let found = conflicts(&registry, &KeymapConfig::default(), id, keys);
        assert!(
            found.same_scope.is_empty() && found.shadows.is_empty(),
            "{id} on {keys} collides: {:?} same-scope, {} shadows",
            found.same_scope,
            found.shadows.len()
        );
        assert!(!found.any_clash(), "{id} on {keys} clashes: {found:?}");
    }
}

/// `settings.show_layouts` registers with no default binding at all - `keymap::
/// bindings` skips it (a palette-only command), so there is nothing for it to
/// collide with. Asserted here rather than silently assumed.
#[test]
fn settings_show_layouts_has_no_default_key() {
    let registry = app_registry();
    let command = registry
        .get("settings.show_layouts")
        .expect("registered app-wide");
    assert_eq!(command.default_binding, "");
}

/// 7.2: the four picker-scoped commands are offered only while
/// `SavedLayoutsPicker` is on the focus-path's context stack.
#[test]
fn picker_commands_are_offered_only_with_the_picker_context_active() {
    let registry = app_registry();
    let picker_commands = [
        "saved_layouts.load_replace",
        "saved_layouts.load_add",
        "saved_layouts.rename_selected",
        "saved_layouts.delete_selected",
    ];
    for id in picker_commands {
        assert!(
            !registry.available(&[]).iter().any(|c| c.id == id),
            "{id} is not offered with no context active"
        );
        assert!(
            registry
                .available(&[crate::ui::picker::saved_layouts::KEY_CONTEXT])
                .iter()
                .any(|c| c.id == id),
            "{id} is offered with SavedLayoutsPicker active"
        );
        assert!(
            !registry
                .available(&[crate::ui::settings::KEY_CONTEXT])
                .iter()
                .any(|c| c.id == id),
            "{id} is not offered just because SettingsWindow is active"
        );
    }
}

/// 7.2: `settings.show_layouts` is offered only while `SettingsWindow` is on
/// the focus-path's context stack - not while the picker's is, and not with
/// neither active.
#[test]
fn settings_show_layouts_is_offered_only_with_the_settings_context_active() {
    let registry = app_registry();
    assert!(
        !registry
            .available(&[])
            .iter()
            .any(|c| c.id == "settings.show_layouts")
    );
    assert!(
        !registry
            .available(&[crate::ui::picker::saved_layouts::KEY_CONTEXT])
            .iter()
            .any(|c| c.id == "settings.show_layouts")
    );
    assert!(
        registry
            .available(&[crate::ui::settings::KEY_CONTEXT])
            .iter()
            .any(|c| c.id == "settings.show_layouts")
    );
}

/// 7.2: `layouts.save` and `layouts.manage` are each offered everywhere
/// (`context: None`), as the palette's catch-all requires for a command with
/// a Window-menu entry (`ui::menu::tests` covers the menu side).
#[test]
fn the_two_top_level_commands_are_offered_with_no_context_active() {
    let registry = app_registry();
    for id in ["layouts.save", "layouts.manage"] {
        assert!(
            registry.available(&[]).iter().any(|c| c.id == id),
            "{id} is offered with no context active"
        );
    }
}
