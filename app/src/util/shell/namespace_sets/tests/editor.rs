//! The set editor and deleting a set (`namespace-sets` 2.2, 3.1-3.4, 6.1): a
//! new set from the focused list's scope, refusals, cancelling, adding a
//! namespace, an absent namespace, and a delete that asks first.

use super::*;
use crate::ui::namespace_filter::{absent_selector, checked_selector};
use crate::ui::namespace_sets::RemoveNamespaceSet;
use crate::ui::namespace_sets::editor::ERROR_SELECTOR;
use crate::ui::namespace_sets::picker::{CONFIRM_DELETE_ID, row_selector};
use gpui_kit::Modifiers;

fn click(h: &mut Harness, id: &'static str) {
    let center = h
        .vcx
        .update_window(h.window.into(), |_, window, cx| {
            window.render_frame(cx);
            window
                .try_find(gpui_kit::ElementId::Name(id.into()))
                .unwrap_or_else(|| panic!("{id} is drawn"))
                .bounds()
                .center()
        })
        .unwrap();
    h.vcx.simulate_click(center, Modifiers::none());
    h.vcx.run_until_parked();
}

/// Spec: "Name the current scope" - the new set holds the focused list's
/// namespaces, and the list keeps them.
#[gpui_kit::test]
async fn a_new_set_names_the_focused_lists_scope(cx: &mut TestAppContext) {
    let mut h = harness_with(cx, Vec::new(), "");
    // Scope Pods to two namespaces with a set, then forget that set.
    h.vcx.update(|_, cx| {
        NamespaceSets::set_for_test(
            NamespaceSetsConfig {
                sets: vec![set("seed", &["team-a", "team-b"])],
            },
            cx,
        )
    });
    press(&mut h.vcx, "cmd-shift-n");
    press(&mut h.vcx, "1");
    h.vcx
        .update(|_, cx| NamespaceSets::set_for_test(NamespaceSetsConfig::default(), cx));

    press(&mut h.vcx, "cmd-alt-n");
    assert!(dialog_open(&mut h), "the editor opens");
    assert!(drawn(&mut h, checked_selector(Some("team-a"))));
    assert!(drawn(&mut h, checked_selector(Some("team-b"))));
    type_text(&mut h, "team-workloads");
    press(&mut h.vcx, "enter");

    assert!(!dialog_open(&mut h));
    assert_eq!(
        saved(&mut h),
        [("team-workloads".into(), strings(&["team-a", "team-b"]))]
    );
    assert_eq!(
        scope_of(&mut h, "demo", &NavTarget::pods()),
        ["team-a", "team-b"]
    );
}

/// Spec: "Duplicate names are rejected" and "An empty set cannot be saved".
#[gpui_kit::test]
async fn a_taken_name_and_an_empty_set_are_refused(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    // Pods is on all namespaces, so the new set starts empty.
    press(&mut h.vcx, "cmd-alt-n");
    type_text(&mut h, "fresh");
    press(&mut h.vcx, "enter");
    assert!(dialog_open(&mut h), "an empty set isn't saved");
    assert!(drawn(&mut h, ERROR_SELECTOR.to_string()));

    // Tab to the list, pick `payments`, back to the name and take a used one.
    press(&mut h.vcx, "tab");
    type_text(&mut h, "pay");
    press(&mut h.vcx, "enter");
    press(&mut h.vcx, "shift-tab");
    press(&mut h.vcx, "cmd-a");
    type_text(&mut h, "payments");
    press(&mut h.vcx, "enter");
    assert!(dialog_open(&mut h), "a taken name isn't saved");
    assert!(drawn(&mut h, ERROR_SELECTOR.to_string()));
    assert_eq!(saved(&mut h).len(), 2);
}

/// Spec: "Cancelling creates nothing".
#[gpui_kit::test]
async fn escape_from_the_editor_creates_nothing(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    press(&mut h.vcx, "cmd-alt-n");
    type_text(&mut h, "never");
    press(&mut h.vcx, "escape");
    assert!(!dialog_open(&mut h));
    assert_eq!(saved(&mut h).len(), 2);
}

/// Spec: "Add a namespace to a set" (saved at once, filter and all) and
/// "Editing a set leaves switched panels alone" - which also leaves the
/// panel's title reading as a count, not the set's name.
#[gpui_kit::test]
async fn adding_a_namespace_saves_at_once_and_leaves_switched_lists(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    press(&mut h.vcx, "cmd-shift-n");
    press(&mut h.vcx, "1");
    let named = |h: &mut Harness| {
        let namespaces = scope_of(h, "demo", &NavTarget::pods());
        h.vcx
            .update(|_, cx| crate::ui::namespace_picker::scope_label(&namespaces, cx))
    };
    assert_eq!(named(&mut h), "team-workloads", "the title names the set");

    press(&mut h.vcx, "cmd-alt-e");
    press(&mut h.vcx, "enter");
    assert!(dialog_open(&mut h), "the editor opens on the first set");
    press(&mut h.vcx, "tab");
    type_text(&mut h, "team");
    press(&mut h.vcx, "down down enter");
    assert_eq!(
        saved(&mut h)[0].1,
        ["team-a", "team-b", "team-c"],
        "saved with no Save"
    );
    assert!(drawn(&mut h, checked_selector(Some("team-c"))));

    assert_eq!(
        scope_of(&mut h, "demo", &NavTarget::pods()),
        ["team-a", "team-b"],
        "the switched list keeps its namespaces"
    );
    assert_eq!(named(&mut h), "2 namespaces", "and no longer names the set");
}

/// Spec: "A set's namespaces may come from another cluster" - listed, marked
/// absent, and removable.
#[gpui_kit::test]
async fn an_absent_namespace_is_marked_and_removable(cx: &mut TestAppContext) {
    let mut h = harness_with(cx, vec![set("old", &["retired-team", "team-a"])], "");
    press(&mut h.vcx, "cmd-alt-e");
    press(&mut h.vcx, "enter");
    assert!(drawn(&mut h, absent_selector("retired-team")));
    assert!(!drawn(&mut h, absent_selector("team-a")));
    press(&mut h.vcx, "tab");
    type_text(&mut h, "retired");
    press(&mut h.vcx, "enter");
    assert_eq!(saved(&mut h)[0].1, ["team-a"]);
}

/// Spec: "Deletion is confirmed" and "Delete a set", from the palette's
/// command (it has no default key): Escape at the question keeps the set,
/// Delete removes it - and a list switched to it keeps its namespaces.
#[gpui_kit::test]
async fn deleting_asks_first(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    press(&mut h.vcx, "cmd-shift-n");
    press(&mut h.vcx, "2");
    let remove = |h: &mut Harness| {
        h.vcx
            .update(|window, cx| window.dispatch_action(Box::new(RemoveNamespaceSet), cx));
        h.vcx.run_until_parked();
    };

    remove(&mut h);
    assert!(drawn(&mut h, row_selector("payments")));
    press(&mut h.vcx, "down enter");
    press(&mut h.vcx, "escape");
    assert!(!dialog_open(&mut h));
    assert_eq!(saved(&mut h).len(), 2, "cancelled");

    remove(&mut h);
    press(&mut h.vcx, "down enter");
    click(&mut h, CONFIRM_DELETE_ID);
    assert!(!dialog_open(&mut h));
    assert_eq!(
        saved(&mut h),
        [("team-workloads".into(), strings(&["team-a", "team-b"]))]
    );
    assert_eq!(scope_of(&mut h, "demo", &NavTarget::pods()), ["payments"]);
}
