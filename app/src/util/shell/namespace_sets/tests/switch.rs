//! Quick selection and applying a set (`namespace-sets` 4.1-4.3, 5.1, 5.3,
//! 2.1): the picker's digits, arrows, Escape, the no-panel notice, a
//! context-wide apply, and a rebound key.

use super::*;
use crate::ui::namespace_sets::SWITCH_COMMAND_ID;
use crate::ui::namespace_sets::picker::{NOTICE_SELECTOR, digit_selector};

/// Spec: "Two keystrokes to switch" - the second set's digit switches only
/// the focused list.
#[gpui_kit::test]
async fn a_digit_switches_the_focused_list(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    press(&mut h.vcx, "cmd-shift-n");
    assert!(dialog_open(&mut h), "the picker opens");
    assert!(drawn(&mut h, digit_selector(0)) && drawn(&mut h, digit_selector(1)));

    press(&mut h.vcx, "2");
    assert!(!dialog_open(&mut h));
    assert_eq!(scope_of(&mut h, "demo", &NavTarget::pods()), ["payments"]);
    assert!(
        scope_of(&mut h, "demo", &NavTarget::Kind(services())).is_empty(),
        "only the focused list moves"
    );
}

/// Spec: "Switch the focused panel" with a two-namespace set, by arrows and
/// Enter - and a namespace the cluster lacks stays in the scope.
#[gpui_kit::test]
async fn enter_applies_the_highlighted_set_absent_namespaces_and_all(cx: &mut TestAppContext) {
    let mut h = harness_with(
        cx,
        vec![
            set("payments", &["payments"]),
            set("team-workloads", &["retired-team", "team-a"]),
        ],
        "",
    );
    press(&mut h.vcx, "cmd-shift-n");
    press(&mut h.vcx, "down");
    press(&mut h.vcx, "enter");
    assert_eq!(
        scope_of(&mut h, "demo", &NavTarget::pods()),
        ["retired-team", "team-a"]
    );
    assert_eq!(saved(&mut h)[1].1, ["retired-team", "team-a"]);
}

/// Spec: "The tenth set has no digit" - it's still reached by arrows and
/// Enter.
#[gpui_kit::test]
async fn the_tenth_set_has_no_digit_but_is_reachable(cx: &mut TestAppContext) {
    let sets: Vec<_> = (1..=10)
        .map(|n| set(&format!("set-{n}"), &[&format!("ns-{n}")]))
        .collect();
    let mut h = harness_with(cx, sets, "");
    press(&mut h.vcx, "cmd-shift-n");
    assert!(drawn(&mut h, digit_selector(8)), "the ninth has `9`");
    assert!(!drawn(&mut h, digit_selector(9)), "the tenth has none");
    for _ in 0..9 {
        press(&mut h.vcx, "down");
    }
    press(&mut h.vcx, "enter");
    assert_eq!(scope_of(&mut h, "demo", &NavTarget::pods()), ["ns-10"]);
}

/// Spec: "Escape changes nothing".
#[gpui_kit::test]
async fn escape_closes_and_changes_nothing(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    press(&mut h.vcx, "cmd-shift-n");
    press(&mut h.vcx, "escape");
    assert!(!dialog_open(&mut h));
    assert!(scope_of(&mut h, "demo", &NavTarget::pods()).is_empty());
}

/// Spec: "Digits are shown only where the command works" - from a
/// cluster-scoped list there are no digits, and choosing a set says why
/// nothing happened.
#[gpui_kit::test]
async fn a_cluster_scoped_list_gets_no_digits_and_a_notice(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    // Opening Nodes again shows and focuses the open one.
    let main = h.main.clone();
    h.vcx.update(|window, cx| {
        main.update(cx, |main, cx| {
            main.open_target(NavTarget::Kind(nodes()), window, cx)
        })
    });
    h.vcx.run_until_parked();
    press(&mut h.vcx, "cmd-shift-n");
    assert!(dialog_open(&mut h));
    assert!(!drawn(&mut h, digit_selector(0)), "no digits");
    assert!(drawn(&mut h, NOTICE_SELECTOR.to_string()), "it says why");
    press(&mut h.vcx, "1");
    press(&mut h.vcx, "enter");
    assert!(dialog_open(&mut h), "nothing to apply to, so it stays");
    assert!(scope_of(&mut h, "demo", &NavTarget::Kind(nodes())).is_empty());
}

/// Spec: "Switch every panel in the context", "Switching does not cross
/// contexts", "Cluster-scoped panels are untouched" - and the context's
/// default becomes the set.
#[gpui_kit::test]
async fn applying_to_the_context_moves_its_lists_and_default(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let team = strings(&["team-a", "team-b"]);
    press(&mut h.vcx, "cmd-alt-shift-n");
    press(&mut h.vcx, "1");
    assert_eq!(scope_of(&mut h, "demo", &NavTarget::pods()), team);
    assert_eq!(scope_of(&mut h, "demo", &NavTarget::Kind(services())), team);
    assert!(scope_of(&mut h, "demo", &NavTarget::Kind(nodes())).is_empty());
    assert!(scope_of(&mut h, "other", &NavTarget::Kind(services())).is_empty());
    assert_eq!(default_of(&mut h, "demo"), Some(team));
    assert_eq!(default_of(&mut h, "other"), None);
}

/// Spec: "Rebinding a namespace-set key" - an override in `keymap.toml`, by
/// id, is the key that opens the picker; the default no longer does.
#[gpui_kit::test]
async fn a_rebound_key_opens_the_picker(cx: &mut TestAppContext) {
    let keymap = format!("[bindings]\n\"{SWITCH_COMMAND_ID}\" = \"cmd-shift-m\"\n");
    let mut h = harness_with(cx, vec![set("payments", &["payments"])], &keymap);
    press(&mut h.vcx, "cmd-shift-n");
    assert!(!dialog_open(&mut h), "the default key is gone");
    press(&mut h.vcx, "cmd-shift-m");
    assert!(dialog_open(&mut h), "the rebound key opens it");
}

/// Every set command is in a generated `keymap.toml`, by id.
#[test]
fn a_generated_keymap_lists_every_set_command() {
    use crate::ui::namespace_sets::*;
    let path = temp_workspace_path();
    let mut registry = crate::command::CommandRegistry::new();
    register_commands(&mut registry);
    let keymap = crate::keymap::load(&path, &registry);
    for id in [
        CREATE_COMMAND_ID,
        EDIT_COMMAND_ID,
        REMOVE_COMMAND_ID,
        SWITCH_COMMAND_ID,
        SWITCH_CONTEXT_COMMAND_ID,
    ] {
        assert!(keymap.bindings.contains_key(id), "{id}");
    }
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains(SWITCH_COMMAND_ID)
    );
}
