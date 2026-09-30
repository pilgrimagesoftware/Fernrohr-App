//! Tests for the Keyboard Shortcuts row model, over a small registry.

use super::{matches, rows, scope_name};
use crate::command::{Command, CommandRegistry};
use crate::keymap::KeymapConfig;
use gpui_kit::actions;

actions!(rows_test, [Global, Panel, PaletteOnly]);

fn registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    for (id, title, default_binding, context, action) in [
        (
            "test.global",
            "Global Thing",
            "cmd-g",
            None,
            Box::new(Global) as Box<dyn gpui_kit::Action>,
        ),
        (
            "test.panel",
            "Panel Thing",
            "d",
            Some("PodsPanel"),
            Box::new(Panel),
        ),
        (
            "test.palette",
            "Palette Only",
            "",
            None,
            Box::new(PaletteOnly),
        ),
    ] {
        registry.register(Command {
            id,
            title,
            default_binding,
            context,
            action,
            menu: None,
        });
    }
    registry
}

#[test]
fn every_command_has_a_row_with_its_key_and_scope() {
    let rows = rows(&registry(), &KeymapConfig::default());
    let summary: Vec<_> = rows
        .iter()
        .map(|row| (row.id, row.scope.as_str(), row.keys.as_deref(), row.changed))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("test.global", "Global", Some("cmd-g"), false),
            ("test.panel", "Pods panel", Some("d"), false),
            ("test.palette", "Global", None, false),
        ]
    );
}

#[test]
fn an_override_or_a_removed_key_is_marked_changed() {
    let mut config = KeymapConfig::default();
    config.bindings.insert("test.global".into(), "cmd-j".into());
    config.bindings.insert("test.panel".into(), String::new());
    let rows = rows(&registry(), &config);
    assert_eq!(rows[0].keys.as_deref(), Some("cmd-j"));
    assert!(rows[0].changed);
    assert_eq!(rows[1].keys, None, "a removed key shows none");
    assert!(rows[1].changed);
}

#[test]
fn a_palette_only_command_given_a_key_is_changed() {
    let mut config = KeymapConfig::default();
    config
        .bindings
        .insert("test.palette".into(), "cmd-k".into());
    let rows = rows(&registry(), &config);
    assert_eq!(rows[2].keys.as_deref(), Some("cmd-k"));
    assert!(rows[2].changed);
}

/// A global and a panel command on one key: both rows say who takes it where.
#[test]
fn a_panel_key_shadowing_a_global_one_is_noted_on_both_rows() {
    let mut config = KeymapConfig::default();
    config.bindings.insert("test.global".into(), "cmd-d".into());
    config.bindings.insert("test.panel".into(), "cmd-d".into());
    let rows = rows(&registry(), &config);
    assert_eq!(
        rows[0].notes,
        vec!["In the Pods panel, “Panel Thing” takes this key.".to_string()]
    );
    assert_eq!(
        rows[1].notes,
        vec!["In the Pods panel, this takes the key from “Global Thing”.".to_string()]
    );
}

#[test]
fn a_plain_key_on_a_global_command_is_warned_about() {
    let mut config = KeymapConfig::default();
    config.bindings.insert("test.global".into(), "q".into());
    let rows = rows(&registry(), &config);
    assert_eq!(rows[0].notes.len(), 1);
    assert!(
        rows[1].notes.is_empty(),
        "a plain panel key is the panel's own style"
    );
}

#[test]
fn the_filter_matches_title_id_or_key() {
    let rows = rows(&registry(), &KeymapConfig::default());
    let found = |query: &str| -> Vec<&str> {
        rows.iter()
            .filter(|row| matches(row, query))
            .map(|row| row.id)
            .collect()
    };
    assert_eq!(found("panel th"), vec!["test.panel"], "title");
    assert_eq!(found("TEST.PALETTE"), vec!["test.palette"], "id, any case");
    assert_eq!(found("cmd-g"), vec!["test.global"], "key");
    assert_eq!(
        found("  "),
        vec!["test.global", "test.panel", "test.palette"]
    );
}

#[test]
fn scope_names_read_as_words() {
    assert_eq!(scope_name(None), "Global");
    assert_eq!(scope_name(Some("PodsPanel")), "Pods panel");
    assert_eq!(scope_name(Some("PodDetailPanel")), "Pod Detail panel");
    assert_eq!(scope_name(Some("KeyboardShortcuts")), "Keyboard Shortcuts");
}
