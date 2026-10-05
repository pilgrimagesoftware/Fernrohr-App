//! `open-in-background` 3.1: a `cmd`-click on a pod's owner link opens the owner
//! as an inactive tab, focus kept in the pod's detail panel; a middle-click on the
//! same link, now that the owner is open, changes nothing.

use super::super::{MainWindow, OpenPanel, WindowMode};
use super::tests::{follow, is_pod, is_showing, open_matching};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::discovery_registry::DiscoveryRegistry;
use crate::k8s::object_ref::ObjectRef;
use crate::ui::nav::{NavTarget, ObjectTarget, OpenedPanel};
use crate::util::shell::test_support::connected_window;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, ElementId, Focusable as _, InputEvent as _, Modifiers, MouseButton,
    MouseDownEvent, MouseUpEvent, TestAppContext, WindowHandle,
};
use k8s_openapi::api::core::v1::Pod;
use kube::core::GroupVersionKind;

fn replica_sets() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("apps", "v1", "ReplicaSet"),
        plural: "replicasets".into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

/// A click on the pod panel's first "Controlled By" link.
fn click_owner(
    cx: &mut TestAppContext,
    window: &WindowHandle<MainWindow>,
    button: MouseButton,
    modifiers: Modifiers,
) {
    cx.update_window((*window).into(), |_, window, cx| {
        window.render_frame(cx);
        let at = window
            .find(ElementId::NamedInteger("Controlled By".into(), 0))
            .bounds()
            .center();
        window.dispatch_event(
            MouseDownEvent {
                button,
                position: at,
                modifiers,
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseUpEvent {
                button,
                position: at,
                modifiers,
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
}

/// Whether focus is in the pod's detail panel.
fn pod_has_focus(cx: &mut TestAppContext, window: &WindowHandle<MainWindow>) -> bool {
    window
        .update(cx, |main_window, window, cx| {
            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                return false;
            };
            open_panels.iter().any(|open| match &open.panel {
                Some(OpenedPanel::PodDetail(panel)) => {
                    panel.read(cx).focus_handle(cx).contains_focused(window, cx)
                }
                _ => false,
            })
        })
        .unwrap()
}

#[gpui_kit::test]
async fn a_cmd_clicked_owner_link_opens_in_the_background(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    cx.update(|cx| DiscoveryRegistry::insert_test(cx, "kind-dev", vec![replica_sets()]));
    follow(
        cx,
        &window,
        "kind-dev",
        ObjectRef::core("Pod", "staging", "web-1"),
    );
    let pod: Pod = serde_json::from_value(serde_json::json!({
        "apiVersion": "v1", "kind": "Pod",
        "metadata": {
            "name": "web-1", "namespace": "staging",
            "ownerReferences": [{
                "apiVersion": "apps/v1", "kind": "ReplicaSet", "name": "web-7d9f",
                "uid": "rs-1", "controller": true,
            }],
        },
    }))
    .unwrap();
    window
        .update(cx, |main_window, window, cx| {
            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            let panel = open_panels
                .iter()
                .find_map(|open| match &open.panel {
                    Some(OpenedPanel::PodDetail(panel)) => Some(panel.clone()),
                    _ => None,
                })
                .expect("the pod's detail panel is open");
            panel.update(cx, |panel, cx| panel.test_set_loaded(pod, cx));
            let focus = panel.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    cx.run_until_parked();
    let pod_panel = open_matching(cx, &window, |open| is_pod(open, "staging", "web-1"))[0].0;
    let is_owner = |open: &OpenPanel| {
        open.key.target
            == NavTarget::Object(ObjectTarget {
                kind: replica_sets(),
                namespace: Some("staging".into()),
                name: "web-7d9f".into(),
            })
    };

    click_owner(cx, &window, MouseButton::Left, Modifiers::secondary_key());

    let owner = open_matching(cx, &window, is_owner);
    assert_eq!(owner.len(), 1, "the ReplicaSet's panel opened");
    assert!(!is_showing(cx, &window, owner[0].0), "as an inactive tab");
    assert!(
        is_showing(cx, &window, pod_panel),
        "the pod's panel still shows"
    );
    assert!(pod_has_focus(cx, &window), "and keeps focus");

    click_owner(cx, &window, MouseButton::Middle, Modifiers::none());

    assert_eq!(
        open_matching(cx, &window, is_owner).len(),
        1,
        "no second panel"
    );
    assert!(!is_showing(cx, &window, owner[0].0), "its tab wasn't shown");
    assert!(pod_has_focus(cx, &window));
}
