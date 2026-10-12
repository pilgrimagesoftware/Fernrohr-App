//! `selector-logs-panel` 5.1: a typed label panel's namespace scope - taken
//! from the list it was opened from, switched by a namespace set, and picked
//! from its bar by keyboard - decides which pods stream.

use super::{logs_panel, wait_for_streams, window};
use crate::config::namespaces::{NamespaceSetConfig, NamespaceSetsConfig};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::namespaces::{NamespaceList, NamespaceRegistry};
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::namespace_filter::option_selector;
use crate::ui::namespace_sets::store::NamespaceSets;
use crate::ui::nav::NavTarget;
use crate::util::logs::LabelLogs;
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
            .filter(|open| open.key.target == NavTarget::LabelLogs(LabelLogs::Typed))
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
    let typed = NavTarget::LabelLogs(LabelLogs::Typed);
    let panel = logs_panel(&main, &typed, &mut vcx);
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

    // Reopening focuses this panel, scope and all, rather than a second.
    vcx.update(|window, cx| window.dispatch_action(Box::new(crate::util::logs::ShowLabelLogs), cx));
    vcx.run_until_parked();
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
