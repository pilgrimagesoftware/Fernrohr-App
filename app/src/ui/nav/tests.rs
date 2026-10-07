use super::{NavTarget, SHOW_EVENTS_COMMAND_ID, SHOW_LOGS_COMMAND_ID, SHOW_PODS_COMMAND_ID};
use crate::command::{CommandRegistry, build_items};
use crate::k8s::cluster::discovery::DiscoveredKind;
use gpui_kit::TestAppContext;
use kube::core::GroupVersionKind;

fn kind(group: &str, kind: &str) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk(group, "v1", kind),
        plural: format!("{}s", kind.to_lowercase()),
        namespaced: true,
        verbs: Default::default(),
    }
}

/// Section 1.1: a pod's detail panel is keyed on *which* pod, so two
/// different pods are two different targets and the same pod is one. The
/// equality is what `PanelKey`'s dedup reads - same key focuses, different
/// key opens a second panel - so it has to be exactly this.
#[test]
fn two_pods_are_different_targets_and_one_pod_is_one_target() {
    let first = NavTarget::pod("default", "web-1");
    let second = NavTarget::pod("default", "web-2");
    let same = NavTarget::pod("default", "web-1");

    assert_ne!(first, second, "different pods are different panels");
    assert_eq!(first, same, "the same pod is the same panel");
}

/// A pod's namespace is part of its identity: the same name in two
/// namespaces is two pods, and collapsing them would make a second cluster
/// namespace's panel unreachable behind the first one's.
#[test]
fn a_pods_namespace_is_part_of_its_identity() {
    assert_ne!(
        NavTarget::pod("default", "web-1"),
        NavTarget::pod("staging", "web-1")
    );
}

/// The list of pods and one pod are not the same panel, whichever way round
/// they are compared - a detail request must never focus the Pods table.
#[test]
fn a_pod_is_not_the_list_of_pods() {
    assert_ne!(NavTarget::pods(), NavTarget::pod("default", "web-1"));
    assert_ne!(NavTarget::pod("default", "web-1"), NavTarget::pods());
}

/// Section 2.1/2.2, read off the target itself: a list titles itself
/// plural, a single item titles itself with the item's name.
#[test]
fn a_target_says_which_way_it_should_be_titled() {
    assert_eq!(NavTarget::pods().list_label(), "Pods");
    assert_eq!(NavTarget::pods().item_label(), "Pods");

    let pod = NavTarget::pod("default", "api-7d9f-ftg5t");
    assert_eq!(pod.label(), "Pod");
    assert_eq!(pod.item_label(), "Pod: api-7d9f-ftg5t");
}

/// Only the core Pod kind keeps the Pods panel; every other kind - a CRD's
/// own `Pod` in its group included - gets the generic list.
#[test]
fn only_the_core_pod_kind_is_the_pods_panel() {
    assert!(DiscoveredKind::pods().is_core_pod());
    assert!(!kind("", "Service").is_core_pod());
    assert!(
        !kind("example.com", "Pod").is_core_pod(),
        "a CRD's own Pod kind is not the built-in Pods panel"
    );
}

/// Selecting Pods and Logs both name a real target, so the palette entries
/// read the same as the Resource panel's rows.
#[test]
fn targets_label_themselves() {
    assert_eq!(NavTarget::pods().label(), "Pod");
    assert_eq!(NavTarget::Logs.label(), "Logs");
    assert_eq!(kind("apps", "Deployment").label(), "Deployment · apps");
}

/// Section 8.3: both panel-opening actions are registered, and both show up
/// in the palette items built from the registry.
#[test]
fn panel_opening_actions_are_registered_commands() {
    let mut registry = CommandRegistry::new();
    super::register_commands(&mut registry);

    assert!(registry.get(SHOW_PODS_COMMAND_ID).is_some());
    assert!(registry.get(SHOW_LOGS_COMMAND_ID).is_some());
    assert!(registry.get("nav.does_not_exist").is_none());

    let titles: Vec<&str> = [SHOW_PODS_COMMAND_ID, SHOW_LOGS_COMMAND_ID]
        .iter()
        .map(|id| registry.get(id).expect("registered above").title)
        .collect();
    assert_eq!(titles, vec!["Show Pods", "Show Logs"]);
    assert!(registry.get(SHOW_EVENTS_COMMAND_ID).is_some());
    assert_eq!(
        build_items(&registry, &[]).len(),
        4,
        "Pods, Logs (both ways) and Events reach the palette"
    );
}

/// Section 8.3: both panel-opening actions dispatch. A dispatched action
/// is what either the palette or the keymap ends up producing, so this is
/// the path a click and a keystroke share.
#[gpui_kit::test]
async fn both_panel_opening_actions_dispatch(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let mut registry = CommandRegistry::new();
    super::register_commands(&mut registry);

    let dispatched_pods = cx.update(|cx| registry.dispatch(SHOW_PODS_COMMAND_ID, &[], cx));
    let dispatched_logs = cx.update(|cx| registry.dispatch(SHOW_LOGS_COMMAND_ID, &[], cx));
    let dispatched = dispatched_pods && dispatched_logs;
    assert!(dispatched, "both commands are registered and ungated");

    assert!(!cx.update(|cx| registry.dispatch("nav.nope", &[], cx)));
}
