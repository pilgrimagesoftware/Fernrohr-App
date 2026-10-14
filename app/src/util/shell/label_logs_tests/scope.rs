//! `selector-logs-panel` 5.1: a typed label panel's namespace scope - taken
//! from the list it was opened from, switched by a namespace set, and picked
//! from its bar by keyboard - decides which pods stream.

use super::{is_typed, typed_panel, wait_for_streams, window};
use crate::config::namespaces::{NamespaceSetConfig, NamespaceSetsConfig};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::namespaces::{NamespaceList, NamespaceRegistry};
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::namespace_filter::option_selector;
use crate::ui::namespace_sets::store::NamespaceSets;
use crate::ui::nav::NavTarget;
use crate::util::shell::{MainWindow, WindowMode};
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};

/// A running pod `name` labelled `app=web` in `namespace`.
fn web_pod(namespace: &str, name: &str) -> serde_json::Value {
    serde_json::json!({
        "apiVersion": "v1", "kind": "Pod",
        "metadata": {
            "name": name, "namespace": namespace, "uid": format!("uid-{name}"),
            "labels": { "app": "web" },
        },
        "status": {
            "phase": "Running",
            "containerStatuses": [{
                "name": "app", "ready": true, "restartCount": 0, "image": "app",
                "imageID": "", "state": { "running": {} },
            }],
        },
    })
}

/// The namespaces the window's typed label panel is keyed - and scoped - to.
fn typed_scope(main: &Entity<MainWindow>, vcx: &mut VisualTestContext) -> Vec<String> {
    vcx.update(|_, cx| {
        let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
            return Vec::new();
        };
        let typed: Vec<_> = open_panels
            .iter()
            .filter(|open| is_typed(&open.key.target))
            .collect();
        assert_eq!(typed.len(), 1, "one typed label panel");
        typed[0].key.namespaces.clone()
    })
}

#[gpui_kit::test]
async fn the_namespace_scope_decides_which_pods_stream(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply("/api/v1", "pods", web_pod("shop", "web-1"));
    cluster.apply("/api/v1", "pods", web_pod("blog", "web-2"));
    cluster.apply("/api/v1", "pods", web_pod("other", "web-3"));
    let (main, mut vcx) = window(ConnectionState::Connected(client), cx);
    vcx.update(|_, cx| {
        let names = cx.new(|_| NamespaceList::with_names(&["blog", "other", "shop"]));
        NamespaceRegistry::set_for_test(cx, "demo", names);
        NamespaceSets::set_for_test(
            NamespaceSetsConfig {
                sets: vec![NamespaceSetConfig {
                    name: "shop+blog".into(),
                    namespaces: vec!["blog".into(), "shop".into()],
                }],
            },
            cx,
        );
    });

    // "Scoped to the current namespace": opened from a Pods list on `shop`.
    vcx.update(|window, cx| {
        main.update(cx, |main, cx| {
            main.open_target_in(
                NavTarget::pods(),
                None,
                None,
                vec!["shop".into()],
                crate::ui::nav::OpenMode::Foreground,
                window,
                cx,
            );
        });
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| window.dispatch_action(Box::new(crate::util::logs::ShowLabelLogs), cx));
    vcx.run_until_parked();
    let panel = typed_panel(&main, &mut vcx);
    assert_eq!(typed_scope(&main, &mut vcx), ["shop"]);
    vcx.simulate_keystrokes("a p p = w e b enter");
    wait_for_streams(&mut vcx, &panel, &["web-1/app#0@uid-web-1"]);

    // "Switching to a set", with the panel focused.
    vcx.update(|window, cx| {
        use gpui_kit::Focusable as _;
        let handle = panel.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
        window.dispatch_action(
            Box::new(crate::ui::namespace_sets::ApplyNamespaceSet {
                name: "shop+blog".into(),
                context_wide: false,
            }),
            cx,
        );
    });
    wait_for_streams(
        &mut vcx,
        &panel,
        &["web-1/app#0@uid-web-1", "web-2/app#0@uid-web-2"],
    );
    assert_eq!(typed_scope(&main, &mut vcx), ["blog", "shop"]);

    // "All namespaces", from the bar's picker by keyboard: `n` opens it on
    // "All namespaces", and Enter picks that.
    vcx.update(|window, cx| {
        use gpui_kit::Focusable as _;
        let handle = panel.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    });
    vcx.simulate_keystrokes("n");
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    const OTHER: &str = "namespace-option other";
    assert_eq!(option_selector(Some("other")), OTHER);
    assert!(
        vcx.debug_bounds(OTHER).is_some(),
        "the picker lists the cluster's namespaces"
    );
    vcx.simulate_keystrokes("enter");
    wait_for_streams(
        &mut vcx,
        &panel,
        &[
            "web-1/app#0@uid-web-1",
            "web-2/app#0@uid-web-2",
            "web-3/app#0@uid-web-3",
        ],
    );
    assert!(typed_scope(&main, &mut vcx).is_empty(), "all namespaces");
}

/// #213 regression: opened from the menu while a Pods list on all namespaces
/// has focus, the typed panel searches every namespace - a selector copied
/// from a pod outside `default` matches it.
#[gpui_kit::test]
async fn opened_from_an_all_namespaces_list_it_matches_any_namespace(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply("/api/v1", "pods", web_pod("blog", "web-2"));
    let (main, mut vcx) = window(ConnectionState::Connected(client), cx);
    vcx.update(|_, cx| {
        let names = cx.new(|_| NamespaceList::with_names(&["blog", "default", "shop"]));
        NamespaceRegistry::set_for_test(cx, "demo", names);
    });
    vcx.update(|window, cx| {
        main.update(cx, |main, cx| {
            main.open_target_in(
                NavTarget::pods(),
                None,
                None,
                Vec::new(),
                crate::ui::nav::OpenMode::Foreground,
                window,
                cx,
            );
        });
    });
    vcx.run_until_parked();
    let mut registry = crate::command::CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    let action = registry
        .get("logs.follow_labels")
        .expect("Open Logs by Label is registered")
        .action
        .boxed_clone();
    vcx.update(|window, cx| window.dispatch_action(action, cx));
    vcx.run_until_parked();
    let panel = typed_panel(&main, &mut vcx);
    assert!(typed_scope(&main, &mut vcx).is_empty(), "all namespaces");
    vcx.simulate_keystrokes("a p p = w e b enter");
    wait_for_streams(&mut vcx, &panel, &["web-2/app#0@uid-web-2"]);
}

/// #213 regression: in a window holding two contexts, the typed panel opens in
/// the context of the list it was opened from - not the window's active one,
/// whose cluster doesn't have the pod the selector was copied from.
#[gpui_kit::test]
async fn it_opens_in_the_focused_lists_context_not_the_active_one(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply("/api/v1", "pods", web_pod("blog", "web-2"));
    let (_other_cluster, other_client) = FakeCluster::start(cx);
    let (main, mut vcx) = window(ConnectionState::Connected(client), cx);
    vcx.update(|window, cx| {
        crate::k8s::cluster::session::ClusterRegistry::insert_test_session(
            cx,
            "other",
            ConnectionState::Connected(other_client),
        );
        main.update(cx, |main, cx| {
            if let WindowMode::Workspace { contexts, .. } = &mut main.mode {
                contexts.push("other".into());
            }
            // `demo`'s Pods list, on all namespaces, has focus while `other`
            // - added later - is the active context.
            main.open_target_in(
                NavTarget::pods(),
                None,
                Some("demo".into()),
                Vec::new(),
                crate::ui::nav::OpenMode::Foreground,
                window,
                cx,
            );
            main.set_active_context("other", window, cx);
        });
    });
    vcx.run_until_parked();
    assert_eq!(
        vcx.update(|_, cx| main.read(cx).test_active_context_name()),
        Some("other".to_string())
    );

    vcx.update(|window, cx| window.dispatch_action(Box::new(crate::util::logs::ShowLabelLogs), cx));
    vcx.run_until_parked();
    let context = vcx.update(|_, cx| {
        let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
            return None;
        };
        open_panels
            .iter()
            .find(|open| is_typed(&open.key.target))
            .map(|open| open.key.context_name.clone())
    });
    assert_eq!(
        context.as_deref(),
        Some("demo"),
        "the focused list's context"
    );
    let panel = typed_panel(&main, &mut vcx);
    vcx.simulate_keystrokes("a p p = w e b enter");
    wait_for_streams(&mut vcx, &panel, &["web-2/app#0@uid-web-2"]);
}
