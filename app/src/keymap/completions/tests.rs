//! `pending-chord-indicator` 1.1, over the app's whole registry: what
//! completes `cmd-k` in a panel group, with a rebound chord, inside a text
//! field, and for keys that start nothing.

use super::{Completion, completions};
use crate::command::CommandRegistry;
use crate::keymap::KeymapConfig;
use gpui_kit::{DummyKeyboardMapper, KeyContext, Keystroke};

fn registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    registry
}

fn keys(keys: &str) -> Vec<Keystroke> {
    keys.split_whitespace()
        .map(|key| Keystroke::parse(key).unwrap())
        .collect()
}

fn contexts(names: &[&str]) -> Vec<KeyContext> {
    names
        .iter()
        .map(|name| KeyContext::parse(name).unwrap())
        .collect()
}

/// Each completion as its remaining keys (`unparse`d) and command id.
fn found(config: &KeymapConfig, pending: &str, stack: &[&str]) -> Vec<(String, &'static str)> {
    completions(
        &registry(),
        config,
        &keys(pending),
        &contexts(stack),
        &DummyKeyboardMapper,
    )
    .into_iter()
    .map(|Completion { remaining, id, .. }| {
        let remaining: Vec<String> = remaining.iter().map(Keystroke::unparse).collect();
        (remaining.join(" "), id)
    })
    .collect()
}

#[test]
fn cmd_k_in_a_panel_group_lists_the_arrange_chords_in_palette_order() {
    assert_eq!(
        found(&KeymapConfig::default(), "cmd-k", &["Workspace", "Dock"]),
        [
            ("left".to_string(), "panel.split_left"),
            ("right".to_string(), "panel.split_right"),
            ("up".to_string(), "panel.split_up"),
            ("down".to_string(), "panel.split_down"),
            ("shift-left".to_string(), "panel.move_left"),
            ("shift-right".to_string(), "panel.move_right"),
            ("shift-up".to_string(), "panel.move_up"),
            ("shift-down".to_string(), "panel.move_down"),
            ("alt-left".to_string(), "panel.merge_left"),
            ("alt-right".to_string(), "panel.merge_right"),
            ("alt-up".to_string(), "panel.merge_up"),
            ("alt-down".to_string(), "panel.merge_down"),
            ("w".to_string(), "panel.close_group"),
        ]
    );
}

#[test]
fn a_completion_carries_its_command_title() {
    let found = completions(
        &registry(),
        &KeymapConfig::default(),
        &keys("cmd-k"),
        &contexts(&["Dock"]),
        &DummyKeyboardMapper,
    );
    assert_eq!(found[12].title, "Panels: Close Group");
}

#[test]
fn a_rebound_chord_lists_its_new_key() {
    let mut config = KeymapConfig::default();
    config
        .bindings
        .insert("panel.split_left".into(), "cmd-k h".into());
    let found = found(&config, "cmd-k", &["Dock"]);
    assert!(found.contains(&("h".to_string(), "panel.split_left")));
    assert!(
        !found.iter().any(|(key, _)| key == "left"),
        "the old key is gone: {found:?}"
    );
}

#[test]
fn a_text_field_excludes_the_dock_chords() {
    assert!(found(&KeymapConfig::default(), "cmd-k", &["Dock", "Input"]).is_empty());
}

#[test]
fn keys_that_start_no_chord_complete_nothing() {
    assert!(found(&KeymapConfig::default(), "cmd-j", &["Dock"]).is_empty());
    assert!(
        found(&KeymapConfig::default(), "cmd-k w", &["Dock"]).is_empty(),
        "a whole chord is complete, not a prefix"
    );
    assert!(found(&KeymapConfig::default(), "", &["Dock"]).is_empty());
}
