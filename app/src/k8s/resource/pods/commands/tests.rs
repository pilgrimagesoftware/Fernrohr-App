//! Tests for the Pods panel's command surface: which commands the palette
//! offers where, and how their keys resolve through the app's real keymap
//! builder (`keymap::bindings`).

use super::{
    DescribePod, PANEL_KEY_CONTEXT, ShowPodLogs, ShowPodYaml, WarpNamespace, register_commands,
};
use crate::command::CommandRegistry;
use crate::keymap::{self, KeymapConfig};
use gpui_kit::{Action, KeyContext, Keymap, Keystroke};

fn registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_commands(&mut registry);
    registry
}

/// The keymap the app binds: every registered command, through its override.
fn app_keymap(registry: &CommandRegistry, config: &KeymapConfig) -> Keymap {
    Keymap::new(keymap::bindings(
        registry,
        config,
        &gpui_kit::DummyKeyboardMapper,
    ))
}

/// Which action `key` resolves to with `contexts` on the focus path, if any.
fn resolve(keymap: &Keymap, key: &str, contexts: &[&str]) -> Vec<Box<dyn Action>> {
    let keystroke = Keystroke::parse(key).expect("a valid keystroke");
    let contexts: Vec<KeyContext> = contexts
        .iter()
        .map(|context| KeyContext::try_from(*context).expect("a valid context name"))
        .collect();
    let (matched, _) = keymap.bindings_for_input(std::slice::from_ref(&keystroke), &contexts);
    matched
        .iter()
        .map(|binding| binding.action().boxed_clone())
        .collect()
}

/// While a Pods panel is on the focus path the palette lists every Pods
/// shortcut, and from any other panel it lists none of them.
#[test]
fn the_palette_offers_the_pods_shortcuts_only_in_a_pods_panel() {
    let registry = registry();

    let in_pods: Vec<&str> = registry
        .available(&[PANEL_KEY_CONTEXT])
        .iter()
        .map(|command| command.id)
        .collect();
    assert_eq!(
        in_pods,
        [
            "pods.quick_look",
            "pods.warp_namespace",
            "pods.warp_all_namespace",
            "pods.describe",
            "pods.logs",
            "pods.yaml",
            "pods.fit_columns",
            "pods.pick_namespaces"
        ]
    );
    assert_eq!(
        crate::command::build_items(&registry, &[PANEL_KEY_CONTEXT]).len(),
        8,
        "each is a palette item"
    );

    assert!(
        registry.available(&["LogsPanel"]).is_empty(),
        "no Pods command is offered from another panel"
    );
}

/// Each default key resolves to the action the hint bar names beside it, with
/// the panel focused or with a table row inside it focused - the case that
/// broke before, because selecting a row moves focus to the table. With another
/// panel focused the key is nobody's shortcut.
#[test]
fn each_default_key_resolves_to_its_action_beneath_a_focused_row() {
    let registry = registry();
    let keymap = app_keymap(&registry, &KeymapConfig::default());
    let expected: [(&str, &dyn Action); 5] = [
        ("w", &WarpNamespace),
        ("d", &DescribePod),
        ("l", &ShowPodLogs),
        ("y", &ShowPodYaml),
        ("n", &crate::ui::namespace_picker::PickNamespaces),
    ];

    for (key, action) in expected {
        for focus_path in [&[PANEL_KEY_CONTEXT][..], &[PANEL_KEY_CONTEXT, "DataTable"]] {
            assert!(
                resolve(&keymap, key, focus_path)
                    .iter()
                    .any(|bound| bound.partial_eq(action)),
                "`{key}` does not resolve to {} with {focus_path:?} focused",
                action.name()
            );
        }
        assert!(
            !resolve(&keymap, key, &["LogsPanel"])
                .iter()
                .any(|bound| bound.partial_eq(action)),
            "`{key}` still fires {} from another panel",
            action.name()
        );
    }
}

/// A `keymap.toml` entry rebinds a Pods shortcut by command id - which the raw
/// bindings these replaced could not, since they never read the keymap.
#[test]
fn a_keymap_override_rebinds_a_pods_shortcut() {
    let registry = registry();
    let mut config = KeymapConfig::default();
    config
        .bindings
        .insert("pods.describe".to_string(), "x".to_string());
    let keymap = app_keymap(&registry, &config);

    assert!(
        resolve(&keymap, "x", &[PANEL_KEY_CONTEXT])
            .iter()
            .any(|bound| bound.partial_eq(&DescribePod)),
        "the override binds `x`"
    );
    assert!(
        !resolve(&keymap, "d", &[PANEL_KEY_CONTEXT])
            .iter()
            .any(|bound| bound.partial_eq(&DescribePod)),
        "and replaces the default rather than adding to it"
    );
}
