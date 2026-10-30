//! #311: the picker lists the saved namespace groups between "All
//! namespaces" and the namespaces, each with its member count; picking one -
//! by keyboard or click - scopes to all of its namespaces, the button names
//! it, and the filter narrows the groups too.

use super::*;
use crate::config::namespaces::{NamespaceSetConfig, NamespaceSetsConfig};
use crate::ui::namespace_filter::{group_checked_selector, group_count_selector, group_selector};
use crate::ui::namespace_picker::scope_label;
use crate::ui::namespace_sets::store::NamespaceSets;

/// The harness, with a group of two namespaces and one naming a namespace
/// the cluster no longer has.
fn with_groups(cx: &mut TestAppContext) -> (Harness, VisualTestContext) {
    let (h, mut vcx) = harness(cx);
    vcx.update(|_, cx| {
        NamespaceSets::set_for_test(
            NamespaceSetsConfig {
                sets: vec![
                    NamespaceSetConfig {
                        name: "kube".into(),
                        namespaces: vec!["kube-public".into(), "kube-system".into()],
                    },
                    NamespaceSetConfig {
                        name: "billing".into(),
                        namespaces: vec!["payments".into(), "retired".into()],
                    },
                ],
            },
            cx,
        );
    });
    vcx.run_until_parked();
    (h, vcx)
}

/// The top of the row `selector` draws.
fn top(vcx: &mut VisualTestContext, h: &Harness, selector: String) -> Pixels {
    let _ = vcx.update_window(h.window.into(), |_, window, cx| window.render_frame(cx));
    vcx.debug_bounds(selector.leak())
        .expect("the row is drawn")
        .top()
}

#[gpui_kit::test]
async fn groups_are_listed_after_all_with_their_counts(cx: &mut TestAppContext) {
    let (h, mut vcx) = with_groups(cx);
    press(&mut vcx, "n");
    for group in ["kube", "billing"] {
        assert!(
            drawn(&mut vcx, &h, group_selector(group)),
            "{group} is listed"
        );
        assert!(
            drawn(&mut vcx, &h, group_count_selector(group)),
            "with its count"
        );
    }
    let all = top(&mut vcx, &h, option_selector(None));
    let kube = top(&mut vcx, &h, group_selector("kube"));
    let billing = top(&mut vcx, &h, group_selector("billing"));
    let first = top(&mut vcx, &h, option_selector(Some("default")));
    assert!(
        all < kube && kube < billing && billing < first,
        "all, the groups, then namespaces"
    );
}

#[gpui_kit::test]
async fn enter_on_a_group_scopes_to_it_and_the_button_names_it(cx: &mut TestAppContext) {
    let (h, mut vcx) = with_groups(cx);
    press(&mut vcx, "n");
    press(&mut vcx, "down");
    press(&mut vcx, "enter");
    assert_eq!(scope(&mut vcx, &h), ["kube-public", "kube-system"]);
    assert!(
        drawn(&mut vcx, &h, group_checked_selector("kube")),
        "checked"
    );
    let label = vcx.update(|_, cx| scope_label(&["kube-public".into(), "kube-system".into()], cx));
    assert_eq!(label, "kube", "the button reads the group's name");
}

#[gpui_kit::test]
async fn a_click_picks_a_group_and_absent_members_match_nothing(cx: &mut TestAppContext) {
    let (h, mut vcx) = with_groups(cx);
    press(&mut vcx, "n");
    click(&mut vcx, &h, group_selector("billing"));
    // `retired` is gone from the cluster: the scope keeps it, as applying the
    // set does, and the namespace that's still there is what shows.
    assert_eq!(scope(&mut vcx, &h), ["payments", "retired"]);
    assert!(drawn(&mut vcx, &h, group_checked_selector("billing")));
}

#[gpui_kit::test]
async fn the_filter_narrows_groups_by_name(cx: &mut TestAppContext) {
    let (h, mut vcx) = with_groups(cx);
    press(&mut vcx, "n");
    type_text(&mut vcx, "bill");
    assert!(drawn(&mut vcx, &h, group_selector("billing")));
    assert!(
        !drawn(&mut vcx, &h, group_selector("kube")),
        "kube doesn't match"
    );
    // The first row after "All namespaces" is the matching group.
    press(&mut vcx, "down");
    press(&mut vcx, "enter");
    assert_eq!(scope(&mut vcx, &h), ["payments", "retired"]);
}
