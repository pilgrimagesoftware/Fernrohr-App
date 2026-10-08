//! Tests for loading, resolving and binding `keymap.toml`.

use super::{KeymapConfig, bindings, load, resolve};
use crate::command::{Command, CommandRegistry};
use gpui_kit::actions;

actions!(keymap_test, [TestAction]);

mod k9s_commands;
mod saved_layouts_commands;

/// `keys` as GPUI spells it back on this platform - `cmd` reads `super` off
/// macOS - for comparing with a keystroke's `unparse`.
fn spelled(keys: &str) -> String {
    gpui_kit::Keystroke::parse(keys)
        .expect("a valid keystroke")
        .unparse()
}

fn temp_path() -> std::path::PathBuf {
    crate::util::test_paths::temp_path("keymap")
}

fn registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    registry.register(Command {
        id: "test.command",
        title: "Test Command",
        default_binding: "cmd-t",
        context: None,
        action: Box::new(TestAction),
        menu: None,
    });
    registry
}

#[test]
fn first_run_writes_every_default_binding() {
    let path = temp_path();
    let registry = registry();

    let keymap = load(&path, &registry);

    assert_eq!(
        keymap.bindings.get("test.command"),
        Some(&"cmd-t".to_string())
    );
    let on_disk: KeymapConfig = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(on_disk, keymap);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn override_rebinds_after_restart() {
    let path = temp_path();
    let registry = registry();
    std::fs::write(&path, "[bindings]\n\"test.command\" = \"cmd-shift-t\"\n").unwrap();

    let keymap = load(&path, &registry);
    let command = registry.get("test.command").unwrap();
    let effective = resolve(command.id, command.default_binding, &keymap);

    assert_eq!(effective, "cmd-shift-t");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn invalid_entry_falls_back_to_default_and_leaves_file_untouched() {
    let path = temp_path();
    let registry = registry();
    let contents = "[bindings]\n\"test.command\" = \"not-a-key!!\"\n";
    std::fs::write(&path, contents).unwrap();

    let keymap = load(&path, &registry);
    let bound = bindings(&registry, &keymap, &gpui_kit::DummyKeyboardMapper);

    assert!(
        bound.iter().any(|binding| binding
            .keystrokes()
            .iter()
            .map(|key| gpui_kit::AsKeystroke::as_keystroke(key).unparse())
            .eq([spelled("cmd-t")])),
        "an unparseable override falls back to the default"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), contents);

    let _ = std::fs::remove_file(&path);
}

/// An entry that is present but empty means "no key" - how the editor's
/// Remove is stored - while an absent entry means "the default".
#[test]
fn an_empty_entry_means_no_key_and_an_absent_one_the_default() {
    let registry = registry();
    let command = registry.get("test.command").unwrap();
    let mut keymap = KeymapConfig::default();
    assert_eq!(
        resolve(command.id, command.default_binding, &keymap),
        "cmd-t"
    );

    keymap.bindings.insert("test.command".into(), "".into());
    assert_eq!(resolve(command.id, command.default_binding, &keymap), "");
    assert!(
        bindings(&registry, &keymap, &gpui_kit::DummyKeyboardMapper).is_empty(),
        "a removed key binds nothing"
    );

    keymap.bindings.insert("test.command".into(), "  ".into());
    assert_eq!(
        resolve(command.id, command.default_binding, &keymap),
        "",
        "whitespace is empty too"
    );
}

#[test]
fn entry_for_unknown_command_id_is_ignored() {
    let path = temp_path();
    let registry = registry();
    std::fs::write(&path, "[bindings]\n\"nonexistent.command\" = \"cmd-x\"\n").unwrap();

    let keymap = load(&path, &registry);
    let command = registry.get("test.command").unwrap();
    let effective = resolve(command.id, command.default_binding, &keymap);

    assert_eq!(effective, "cmd-t");
    let _ = std::fs::remove_file(&path);
}

/// Every registered command with a default gets a key - `tunnels.manage` once
/// had a menu item and palette entry but no binding, because keys came from a
/// hand-kept list. A command missing from an older `keymap.toml` still gets its
/// default. A palette-only command (no default, like About) gets none.
#[test]
fn every_registered_command_is_bound() {
    let mut registry = CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    let keymap = KeymapConfig::default();

    let bound = bindings(&registry, &keymap, &gpui_kit::DummyKeyboardMapper);
    assert_eq!(
        bound.len(),
        registry
            .iter()
            .filter(|command| !command.default_binding.is_empty())
            .count(),
        "one binding per command that has a default"
    );
    assert!(
        registry
            .iter()
            .any(|command| command.default_binding.is_empty()),
        "palette-only commands exist, so the filter above is exercised"
    );
    let manage = bound
        .iter()
        .find(|binding| {
            binding
                .action()
                .partial_eq(&crate::ui::tunnels::TunnelsManage)
        })
        .expect("tunnels.manage is bound");
    let keys: Vec<String> = manage.keystrokes().iter().map(|k| k.unparse()).collect();
    // Compared through the same parse/unparse, not as a literal: `cmd` is the
    // platform modifier, written back as `super` on Linux.
    let expected = gpui_kit::Keystroke::parse(
        registry
            .get("tunnels.manage")
            .expect("registered")
            .default_binding,
    )
    .expect("the default parses")
    .unparse();
    assert_eq!(keys, vec![expected]);
}

mod live {
    //! `keymap::apply`: an edit is saved, and re-binds just its command.
    use crate::command::{Command, CommandRegistry};
    use crate::keymap::{Edit, KeymapConfig, LiveKeymap, apply, bindings, load};
    use gpui_kit::{Action, App, KeyContext, Keystroke, TestAppContext, actions};

    actions!(keymap_live_test, [Target, Other]);

    fn registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        for (id, default_binding, action) in [
            ("test.target", "cmd-t", Box::new(Target) as Box<dyn Action>),
            ("test.other", "cmd-o", Box::new(Other)),
        ] {
            registry.register(Command {
                id,
                title: id,
                default_binding,
                context: None,
                action,
                menu: None,
            });
        }
        registry
    }

    /// An app with `registry`'s keys bound and the live keymap installed, as
    /// `shell::init` leaves it - plus gpui-component's own bindings.
    fn app(cx: &mut TestAppContext) -> std::path::PathBuf {
        let path = super::temp_path();
        let _ = std::fs::remove_file(&path);
        cx.update(|cx| {
            crate::util::test_ui::init(cx);
            let registry = registry();
            let config = load(&path, &registry);
            cx.bind_keys(bindings(&registry, &config, &gpui_kit::DummyKeyboardMapper));
            cx.set_global(LiveKeymap::new(path.clone(), config));
            cx.set_global(registry);
        });
        path
    }

    fn resolves(cx: &App, key: &str, action: &dyn Action, context: &[&str]) -> bool {
        let keystroke = Keystroke::parse(key).expect("a valid keystroke");
        let context: Vec<KeyContext> = context
            .iter()
            .map(|name| KeyContext::try_from(*name).expect("a valid context"))
            .collect();
        let (matched, _) = cx
            .key_bindings()
            .borrow()
            .bindings_for_input(std::slice::from_ref(&keystroke), &context);
        matched
            .iter()
            .any(|binding| binding.action().partial_eq(action))
    }

    #[gpui_kit::test]
    fn set_rebinds_live_and_saves(cx: &mut TestAppContext) {
        let path = app(cx);
        cx.update(|cx| {
            apply(cx, "test.target", Edit::Set("cmd-shift-j".into())).expect("saved");
            assert!(
                resolves(cx, "cmd-shift-j", &Target, &[]),
                "the new key works"
            );
            assert!(
                !resolves(cx, "cmd-t", &Target, &[]),
                "the old key no longer does"
            );
            assert!(
                resolves(cx, "cmd-o", &Other, &[]),
                "another command is untouched"
            );
            assert!(
                resolves(
                    cx,
                    "backspace",
                    &gpui_kit::component::input::Backspace,
                    &["Input"]
                ),
                "gpui-component's own bindings survive - nothing was cleared"
            );
        });
        let saved: KeymapConfig = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved.bindings["test.target"], "cmd-shift-j");
        let _ = std::fs::remove_file(&path);
    }

    #[gpui_kit::test]
    fn remove_leaves_no_key_and_reset_restores_the_default(cx: &mut TestAppContext) {
        let path = app(cx);
        cx.update(|cx| {
            apply(cx, "test.target", Edit::Remove).expect("saved");
            assert!(!resolves(cx, "cmd-t", &Target, &[]), "removed: no key");
            let saved: KeymapConfig =
                toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(
                saved.bindings["test.target"], "",
                "stored as an empty entry"
            );

            apply(cx, "test.target", Edit::Reset).expect("saved");
            assert!(
                resolves(cx, "cmd-t", &Target, &[]),
                "reset: the default again"
            );
            let saved: KeymapConfig =
                toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert!(
                !saved.bindings.contains_key("test.target"),
                "reset drops the entry"
            );
        });
        let _ = std::fs::remove_file(&path);
    }

    /// A key taken back after being moved away works again: the later binding
    /// outranks the earlier `Unbind`.
    #[gpui_kit::test]
    fn moving_a_key_away_and_back_works(cx: &mut TestAppContext) {
        let path = app(cx);
        cx.update(|cx| {
            apply(cx, "test.target", Edit::Set("cmd-shift-j".into())).expect("saved");
            apply(cx, "test.target", Edit::Set("cmd-t".into())).expect("saved");
            assert!(resolves(cx, "cmd-t", &Target, &[]));
            assert!(!resolves(cx, "cmd-shift-j", &Target, &[]));
        });
        let _ = std::fs::remove_file(&path);
    }
}

mod override_precedence {
    //! A `keymap.toml` override for one command beats another command's
    //! default on the same keys - whichever was registered first - both at
    //! load and after a live rebind. `bindings` sorts shorter keys first (for
    //! #137's chords); the override has to survive that sort.
    use crate::command::{Command, CommandRegistry};
    use crate::keymap::{Edit, KeymapConfig, LiveKeymap, apply, bindings};
    use gpui_kit::{Action, App, KeyContext, Keystroke, TestAppContext, actions};

    actions!(keymap_override_test, [Early, Late, ChordEarly, ChordLate]);

    /// `Early` registered before `Late`, and `ChordEarly` before `ChordLate`,
    /// all global.
    fn registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        for (id, default_binding, action) in [
            ("test.early", "cmd-e", Box::new(Early) as Box<dyn Action>),
            ("test.chord_early", "cmd-k e", Box::new(ChordEarly)),
            ("test.late", "cmd-l", Box::new(Late)),
            ("test.chord_late", "cmd-k l", Box::new(ChordLate)),
        ] {
            registry.register(Command {
                id,
                title: id,
                default_binding,
                context: None,
                action,
                menu: None,
            });
        }
        registry
    }

    /// The action `keys` runs: the highest-precedence binding for them.
    fn runs(cx: &App, keys: &str) -> Option<Box<dyn Action>> {
        let keystrokes: Vec<Keystroke> = keys
            .split(' ')
            .map(|key| Keystroke::parse(key).expect("a valid keystroke"))
            .collect();
        let context = [KeyContext::default()];
        let (matched, _) = cx
            .key_bindings()
            .borrow()
            .bindings_for_input(&keystrokes, &context);
        matched
            .first()
            .map(|binding| binding.action().boxed_clone())
    }

    fn app(cx: &mut TestAppContext, keymap_text: &str) -> std::path::PathBuf {
        let path = super::temp_path();
        std::fs::write(&path, keymap_text).expect("written");
        cx.update(|cx| {
            crate::util::test_ui::init(cx);
            let registry = registry();
            let config = crate::keymap::load(&path, &registry);
            cx.bind_keys(bindings(&registry, &config, &gpui_kit::DummyKeyboardMapper));
            cx.set_global(LiveKeymap::new(path.clone(), config));
            cx.set_global(registry);
        });
        path
    }

    fn assert_runs(cx: &App, keys: &str, action: &dyn Action, why: &str) {
        let ran = runs(cx, keys);
        assert!(
            ran.as_ref().is_some_and(|ran| ran.partial_eq(action)),
            "{keys} runs {action:?}, not {ran:?}: {why}"
        );
    }

    /// At load: an override onto a later-registered command's default key -
    /// a single key, and a chord - wins over that default.
    #[gpui_kit::test]
    fn an_override_beats_an_unrelated_default_at_load(cx: &mut TestAppContext) {
        let path = app(
            cx,
            "[bindings]\n\"test.early\" = \"cmd-l\"\n\"test.chord_early\" = \"cmd-k l\"\n",
        );
        cx.update(|cx| {
            assert_runs(cx, "cmd-l", &Early, "the override beats Late's default");
            assert_runs(
                cx,
                "cmd-k l",
                &ChordEarly,
                "the chord override beats ChordLate's default",
            );
        });
        let _ = std::fs::remove_file(&path);
    }

    /// Live, through Settings' path: the same overrides made after launch
    /// win too.
    #[gpui_kit::test]
    fn an_override_beats_an_unrelated_default_after_a_live_rebind(cx: &mut TestAppContext) {
        let path = app(cx, "");
        cx.update(|cx| {
            apply(cx, "test.early", Edit::Set("cmd-l".into())).expect("saved");
            apply(cx, "test.chord_early", Edit::Set("cmd-k l".into())).expect("saved");
            assert_runs(cx, "cmd-l", &Early, "the override beats Late's default");
            assert_runs(
                cx,
                "cmd-k l",
                &ChordEarly,
                "the chord override beats ChordLate's default",
            );
        });
        let saved: KeymapConfig = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved.bindings["test.early"], "cmd-l");
        let _ = std::fs::remove_file(&path);
    }
}

mod conflicts {
    //! `keymap::conflicts`: same-scope collisions, cross-scope shadows, and
    //! keys that start another's chord.
    use crate::command::{Command, CommandRegistry};
    use crate::keymap::{KeymapConfig, Prefix, PrefixKind, Shadow, conflicts, lacks_modifier};
    use gpui_kit::{Action, actions};

    actions!(keymap_conflicts_test, [A, B, InPods, InLogs, InDock]);

    fn registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        for (id, default_binding, context, action) in [
            ("a", "cmd-a", None, Box::new(A) as Box<dyn Action>),
            ("b", "cmd-b", None, Box::new(B)),
            ("pods", "d", Some("PodsPanel"), Box::new(InPods)),
            ("logs", "d", Some("LogsPanel"), Box::new(InLogs)),
            ("dock", "cmd-k right", Some("Dock"), Box::new(InDock)),
        ] {
            registry.register(Command {
                id,
                title: id,
                default_binding,
                context,
                action,
                menu: None,
            });
        }
        registry
    }

    #[test]
    fn two_global_commands_on_one_key_conflict() {
        let found = conflicts(&registry(), &KeymapConfig::default(), "a", "cmd-b");
        assert_eq!(found.same_scope, vec!["b"]);
        assert!(found.shadows.is_empty());
    }

    #[test]
    fn equivalent_spellings_match() {
        let found = conflicts(&registry(), &KeymapConfig::default(), "a", "shift-cmd-b");
        assert!(found.same_scope.is_empty(), "shift-cmd-b isn't cmd-b");
        let mut config = KeymapConfig::default();
        config.bindings.insert("b".into(), "cmd-shift-b".into());
        let found = conflicts(&registry(), &config, "a", "shift-cmd-b");
        assert_eq!(found.same_scope, vec!["b"], "same keys, different order");
    }

    #[test]
    fn different_panels_do_not_conflict() {
        let found = conflicts(&registry(), &KeymapConfig::default(), "pods", "d");
        assert!(
            found.same_scope.is_empty(),
            "Pods and Logs are never both deepest"
        );
        assert!(found.shadows.is_empty());
    }

    #[test]
    fn a_global_key_a_panel_uses_is_a_shadow_not_a_conflict() {
        let found = conflicts(&registry(), &KeymapConfig::default(), "a", "d");
        assert!(found.same_scope.is_empty());
        assert_eq!(
            found.shadows,
            vec![
                Shadow {
                    panel_command: "pods",
                    global_command: "a",
                    context: "PodsPanel"
                },
                Shadow {
                    panel_command: "logs",
                    global_command: "a",
                    context: "LogsPanel"
                },
            ]
        );
    }

    #[test]
    fn a_removed_key_conflicts_with_nothing() {
        let mut config = KeymapConfig::default();
        config.bindings.insert("b".into(), String::new());
        assert!(
            conflicts(&registry(), &config, "a", "cmd-b")
                .same_scope
                .is_empty()
        );
    }

    #[test]
    fn a_key_that_starts_another_chord_clashes() {
        let found = conflicts(&registry(), &KeymapConfig::default(), "a", "cmd-k");
        assert!(found.same_scope.is_empty());
        assert_eq!(
            found.prefixes,
            vec![Prefix {
                command: "dock",
                kind: PrefixKind::StartsTheirs
            }],
            "a global cmd-k would cut Dock's cmd-k right short"
        );
        assert!(found.any_clash());
    }

    #[test]
    fn a_chord_that_another_key_starts_clashes() {
        let found = conflicts(&registry(), &KeymapConfig::default(), "a", "cmd-b x");
        assert_eq!(
            found.prefixes,
            vec![Prefix {
                command: "b",
                kind: PrefixKind::StartsMine
            }],
            "cmd-b fires before the chord's second key"
        );
        let found = conflicts(&registry(), &KeymapConfig::default(), "dock", "cmd-a right");
        assert_eq!(
            found.prefixes,
            vec![Prefix {
                command: "a",
                kind: PrefixKind::StartsMine
            }],
            "a global key overlaps a panel's chord"
        );
    }

    #[test]
    fn prefixes_in_scopes_that_never_overlap_do_not_clash() {
        let found = conflicts(&registry(), &KeymapConfig::default(), "pods", "cmd-k");
        assert!(
            found.prefixes.is_empty(),
            "Pods and Dock are separate contexts"
        );
        assert!(!found.any_clash());
        let found = conflicts(&registry(), &KeymapConfig::default(), "a", "cmd-k right");
        assert!(found.prefixes.is_empty(), "the same chord is not a prefix");
    }

    #[test]
    fn plain_keys_lack_a_modifier() {
        assert!(lacks_modifier("d"));
        assert!(lacks_modifier("shift-d"), "shift alone still types");
        assert!(!lacks_modifier("cmd-d"));
        assert!(!lacks_modifier("ctrl-1"));
    }
}
