//! `k9s-remaining-keybindings` section 8: every command the change adds is
//! registered app-wide with a default key, written to a first-run
//! `keymap.toml`, and offered by the palette where its context is focused -
//! all through the registry, with no per-command keymap or palette code.

use super::super::{KeymapConfig, load};
use crate::command::CommandRegistry;

/// Each new command, and the context names on the focus path where the
/// palette should offer it.
const NEW_COMMANDS: &[(&str, &[&str])] = &[
    ("pods.delete", &["PodsPanel"]),
    ("pods.kill", &["PodsPanel"]),
    ("pods.shell", &["PodsPanel", "PodShellable"]),
    ("pods.port_forward", &["PodsPanel"]),
    (
        "services.port_forward",
        &["ObjectListPanel", "ServicesList"],
    ),
    ("object_detail.edit", &["ObjectDetailPanel"]),
    (
        "object_detail.save_edit",
        &["ObjectDetailPanel", "ObjectYamlEdit"],
    ),
    (
        "object_detail.cancel_edit",
        &["ObjectDetailPanel", "ObjectYamlEdit"],
    ),
    ("logs.toggle_previous", &["LogsPanel"]),
    ("namespaces.jump_all", &["PodsPanel", "NamespacedList"]),
    ("namespaces.jump_1", &["PodsPanel", "NamespacedList"]),
    ("namespaces.jump_5", &["ObjectListPanel", "NamespacedList"]),
    ("namespaces.jump_9", &["PodsPanel", "NamespacedList"]),
    ("global.show_key_hints", &["PodsPanel"]),
];

fn app_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    registry
}

/// 8.1: every new id is registered with a default key, and a first-run
/// `keymap.toml` lists it with that key.
#[test]
fn a_first_run_keymap_lists_every_new_command() {
    let registry = app_registry();
    let path = crate::util::test_paths::temp_path("k9s-commands-keymap");
    let _ = std::fs::remove_file(&path);

    load(&path, &registry);
    let written: KeymapConfig =
        toml::from_str(&std::fs::read_to_string(&path).expect("first run writes keymap.toml"))
            .expect("a keymap");
    for (id, _) in NEW_COMMANDS {
        let command = registry
            .get(id)
            .unwrap_or_else(|| panic!("{id} is registered"));
        assert!(
            !command.default_binding.is_empty(),
            "{id} has a default key"
        );
        assert_eq!(
            written.bindings.get(*id).map(String::as_str),
            Some(command.default_binding),
            "{id} is in the first-run keymap.toml with its key"
        );
    }
    let _ = std::fs::remove_file(&path);
}

/// 8.2, the palette half: each new command is offered where its context is
/// focused - the palette lists `registry.available` for the focused contexts,
/// so nothing per command was needed. The editor half is
/// `ui::settings::tests::every_new_k9s_command_has_an_editor_row`.
#[test]
fn the_palette_offers_every_new_command_in_its_context() {
    let registry = app_registry();
    for (id, contexts) in NEW_COMMANDS {
        assert!(
            registry
                .available(contexts)
                .iter()
                .any(|command| command.id == *id),
            "{id} is in the palette with {contexts:?} focused"
        );
    }
}
