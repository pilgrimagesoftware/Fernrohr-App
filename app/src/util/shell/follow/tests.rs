//! Following references through the window: `resource-links` 3.1-3.3.

use super::super::{MainWindow, OpenPanel, WindowMode};
use crate::k8s::object_ref::ObjectRef;
use crate::ui::link::FollowReference;
use crate::ui::nav::{NavTarget, OpenedPanel};
use crate::util::shell::test_support::connected_window;
use gpui_kit::component::dock::{DockPlacement, PaneRef, PanelId};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, TestAppContext, WindowHandle};
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

fn follow(
    cx: &mut TestAppContext,
    window: &WindowHandle<MainWindow>,
    context: &str,
    target: ObjectRef,
) {
    window
        .update(cx, |main_window, window, cx| {
            main_window.focus_handle.clone().focus(window, cx);
            window.dispatch_action(
                Box::new(FollowReference {
                    context_name: context.into(),
                    target,
                }),
                cx,
            );
        })
        .unwrap();
    cx.run_until_parked();
}

/// The panels this window has open whose key matches `predicate`.
fn open_matching(
    cx: &mut TestAppContext,
    window: &WindowHandle<MainWindow>,
    predicate: impl Fn(&OpenPanel) -> bool,
) -> Vec<(PanelId, String, Vec<String>)> {
    window
        .update(cx, |main_window, _window, _cx| {
            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            open_panels
                .iter()
                .filter(|open| predicate(open))
                .map(|open| {
                    (
                        open.id,
                        open.key.context_name.clone(),
                        open.key.namespaces.clone(),
                    )
                })
                .collect()
        })
        .unwrap()
}

/// Whether `id` is the showing tab of its tab group - what "focused" means for
/// a panel that was already open.
fn is_showing(cx: &mut TestAppContext, window: &WindowHandle<MainWindow>, id: PanelId) -> bool {
    window
        .update(cx, |main_window, _window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            let area = dock_area.read(cx);
            let tree = area.layout(DockPlacement::Center).expect("a centre");
            let node = tree.find_panel_node(id).expect("the panel is docked");
            match tree.find_node(node).expect("its node exists").kind() {
                PaneRef::Tabs { panels, active_ix } => panels.get(active_ix) == Some(&id),
                PaneRef::Split { .. } => false,
            }
        })
        .unwrap()
}

fn is_pod(open: &OpenPanel, namespace: &str, name: &str) -> bool {
    open.key.target == NavTarget::pod(namespace, name)
}

fn is_scoped_pods_list(open: &OpenPanel, namespace: &str) -> bool {
    open.key.target == NavTarget::pods() && open.key.namespaces == [namespace.to_string()]
}

/// 3.1: following a Pod reference opens its detail panel; following it again,
/// after something else took focus, focuses that same panel instead of adding a
/// second.
#[gpui_kit::test]
async fn following_a_pod_opens_it_and_following_again_focuses_it(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    let pod = ObjectRef::core("Pod", "staging", "web-1");

    follow(cx, &window, "kind-dev", pod.clone());
    let opened = open_matching(cx, &window, |open| is_pod(open, "staging", "web-1"));
    assert_eq!(opened.len(), 1, "the pod's detail panel opened");
    let (pod_panel, context, _) = opened[0].clone();
    assert_eq!(context, "kind-dev");
    assert!(is_showing(cx, &window, pod_panel), "and took focus");

    // Something else takes focus...
    follow(
        cx,
        &window,
        "kind-dev",
        ObjectRef::cluster_scoped("", "Namespace", "staging"),
    );
    assert!(!is_showing(cx, &window, pod_panel));

    // ...and following the pod again brings its one panel back.
    follow(cx, &window, "kind-dev", pod);
    let opened = open_matching(cx, &window, |open| is_pod(open, "staging", "web-1"));
    assert_eq!(opened.len(), 1, "no second panel for the same pod");
    assert!(
        is_showing(cx, &window, pod_panel),
        "the existing panel is focused"
    );
}

/// 3.1: a reference is followed in the context of the panel it was shown in,
/// not the window's active one - and a context the window doesn't hold is
/// refused rather than swapped for the active one.
#[gpui_kit::test]
async fn following_uses_the_source_panels_context(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    window
        .update(cx, |main_window, window, cx| {
            main_window.add_context("staging".to_string(), window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        window
            .update(cx, |main_window, _, _| main_window
                .test_active_context_name())
            .unwrap()
            .as_deref(),
        Some("staging"),
        "the second context is the active one"
    );

    follow(
        cx,
        &window,
        "kind-dev",
        ObjectRef::core("Pod", "prod", "web-1"),
    );
    let opened = open_matching(cx, &window, |open| is_pod(open, "prod", "web-1"));
    assert_eq!(opened.len(), 1);
    assert_eq!(
        opened[0].1, "kind-dev",
        "the source panel's context, not the active one"
    );

    follow(
        cx,
        &window,
        "elsewhere",
        ObjectRef::core("Pod", "prod", "api-1"),
    );
    assert!(
        open_matching(cx, &window, |open| is_pod(open, "prod", "api-1")).is_empty(),
        "a context this window doesn't hold opens nothing"
    );
}

/// 3.3: a Namespace reference opens the Pods list scoped to that namespace -
/// a panel of its own, beside the unscoped list - and following it again
/// focuses that scoped list.
#[gpui_kit::test]
async fn following_a_namespace_opens_the_pods_list_scoped_to_it(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    let namespace = ObjectRef::cluster_scoped("", "Namespace", "staging");

    follow(cx, &window, "kind-dev", namespace.clone());
    follow(cx, &window, "kind-dev", namespace);

    let scoped = open_matching(cx, &window, |open| is_scoped_pods_list(open, "staging"));
    assert_eq!(scoped.len(), 1, "one Pods list scoped to staging");
    assert_eq!(scoped[0].2, vec!["staging".to_string()]);
    assert!(is_showing(cx, &window, scoped[0].0));
}

/// 3.2: clicking a reference in a pod's detail panel opens (and focuses) its
/// target, through the real click path.
#[gpui_kit::test]
async fn clicking_a_reference_in_a_pod_panel_opens_its_target(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    follow(
        cx,
        &window,
        "kind-dev",
        ObjectRef::core("Pod", "staging", "web-1"),
    );

    let pod = Pod {
        metadata: ObjectMeta {
            name: Some("web-1".into()),
            namespace: Some("staging".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    window
        .update(cx, |main_window, _window, cx| {
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
        })
        .unwrap();
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click(ElementId::NamedInteger("Namespace".into(), 0), cx);
    })
    .unwrap();
    cx.run_until_parked();

    let scoped = open_matching(cx, &window, |open| is_scoped_pods_list(open, "staging"));
    assert_eq!(scoped.len(), 1, "the namespace's Pods list opened");
    assert!(is_showing(cx, &window, scoped[0].0), "and took focus");
}

/// 5.4: once the context's discovery reports ReplicaSets, following a
/// ReplicaSet reference opens the generic object viewer over it - one panel,
/// focused again rather than duplicated when followed a second time.
#[gpui_kit::test]
async fn following_a_discovered_kind_opens_one_object_panel(cx: &mut TestAppContext) {
    use crate::k8s::cluster::discovery::DiscoveredKind;
    use crate::k8s::cluster::discovery_registry::DiscoveryRegistry;
    use kube::core::GroupVersionKind;

    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    let replica_sets = DiscoveredKind {
        gvk: GroupVersionKind::gvk("apps", "v1", "ReplicaSet"),
        plural: "replicasets".into(),
        namespaced: true,
        verbs: Default::default(),
    };
    cx.update(|cx| DiscoveryRegistry::insert_test(cx, "kind-dev", vec![replica_sets.clone()]));
    let owner = ObjectRef::namespaced("apps", "ReplicaSet", "staging", "web-7d9f");
    let is_owner_panel = |open: &OpenPanel| {
        open.key.target
            == NavTarget::Object(crate::ui::nav::ObjectTarget {
                kind: replica_sets.clone(),
                namespace: Some("staging".into()),
                name: "web-7d9f".into(),
            })
    };

    follow(cx, &window, "kind-dev", owner.clone());
    let opened = open_matching(cx, &window, is_owner_panel);
    assert_eq!(opened.len(), 1, "the ReplicaSet's panel opened");
    let panel = opened[0].0;

    follow(
        cx,
        &window,
        "kind-dev",
        ObjectRef::cluster_scoped("", "Namespace", "staging"),
    );
    follow(cx, &window, "kind-dev", owner);
    assert_eq!(
        open_matching(cx, &window, is_owner_panel).len(),
        1,
        "no second panel for the same object"
    );
    assert!(
        is_showing(cx, &window, panel),
        "the existing panel is focused"
    );
}

/// Dispatches what activating a list row does: `OpenListedObject` for `target` in
/// `context`.
fn open_listed(
    cx: &mut TestAppContext,
    window: &WindowHandle<MainWindow>,
    context: &str,
    target: crate::ui::nav::ObjectTarget,
) {
    window
        .update(cx, |main_window, window, cx| {
            main_window.focus_handle.clone().focus(window, cx);
            window.dispatch_action(
                Box::new(crate::k8s::resource::object_list::OpenListedObject {
                    context_name: context.into(),
                    target,
                    view: None,
                    mode: crate::ui::nav::OpenMode::Foreground,
                }),
                cx,
            );
        })
        .unwrap();
    cx.run_until_parked();
}

/// `standard-resource-panels` 1.5: activating a list row opens that object's detail
/// panel; activating it again, after something else took focus, focuses the same
/// panel rather than opening a second.
#[gpui_kit::test]
async fn activating_a_listed_object_again_focuses_its_panel(cx: &mut TestAppContext) {
    use crate::k8s::cluster::discovery::DiscoveredKind;
    use kube::core::GroupVersionKind;

    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    let target = crate::ui::nav::ObjectTarget {
        kind: DiscoveredKind {
            gvk: GroupVersionKind::gvk("", "v1", "Service"),
            plural: "services".into(),
            namespaced: true,
            verbs: Default::default(),
        },
        namespace: Some("staging".into()),
        name: "web".into(),
    };
    let is_service_panel = {
        let target = target.clone();
        move |open: &OpenPanel| open.key.target == NavTarget::Object(target.clone())
    };

    open_listed(cx, &window, "kind-dev", target.clone());
    let opened = open_matching(cx, &window, &is_service_panel);
    assert_eq!(opened.len(), 1, "the Service's detail panel opened");
    let (panel, context, _) = opened[0].clone();
    assert_eq!(context, "kind-dev", "in the list's context");
    assert!(is_showing(cx, &window, panel));

    // Something else takes focus...
    follow(
        cx,
        &window,
        "kind-dev",
        ObjectRef::cluster_scoped("", "Namespace", "staging"),
    );
    assert!(!is_showing(cx, &window, panel));

    // ...and activating the row again brings its one panel back.
    open_listed(cx, &window, "kind-dev", target);
    assert_eq!(
        open_matching(cx, &window, &is_service_panel).len(),
        1,
        "no second panel for the same object"
    );
    assert!(
        is_showing(cx, &window, panel),
        "the existing panel is focused"
    );
}

/// A row of a Namespaces list opens that Namespace's detail panel - unlike a
/// followed reference to it, which opens the Pods list scoped to it.
#[gpui_kit::test]
async fn activating_a_namespace_row_opens_its_detail_not_the_pods_list(cx: &mut TestAppContext) {
    use crate::k8s::cluster::discovery::DiscoveredKind;
    use kube::core::GroupVersionKind;

    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    let target = crate::ui::nav::ObjectTarget {
        kind: DiscoveredKind {
            gvk: GroupVersionKind::gvk("", "v1", "Namespace"),
            plural: "namespaces".into(),
            namespaced: false,
            verbs: Default::default(),
        },
        namespace: None,
        name: "staging".into(),
    };
    open_listed(cx, &window, "kind-dev", target.clone());
    let opened = open_matching(cx, &window, |open| {
        open.key.target == NavTarget::Object(target.clone())
    });
    assert_eq!(opened.len(), 1, "the Namespace's own detail panel");
}
