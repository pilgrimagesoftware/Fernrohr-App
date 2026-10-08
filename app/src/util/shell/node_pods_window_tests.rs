//! #186 through a real window: Enter, `d` and `l` on a row in a Node's
//! embedded pods region open that pod's detail or logs through the same
//! window actions (`ShowPodDetail`, `ShowLogs`) and `SelectedPod` global
//! every pod list opens through - in the node's own cluster context, dock
//! panel and all. The region's content and search are
//! `object_detail::tests::node_pods`'s instead: that bare `Root` has no dock
//! to assert a panel opened in.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::SelectedPod;
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::nav::{NavTarget, ObjectTarget, OpenedPanel, PodRef};
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, WindowMode, init};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, Focusable as _, TestAppContext, VisualTestContext};
use kube::core::GroupVersionKind;
use serde_json::json;

const CONTEXT: &str = "node-pods-window";

fn node_kind() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "Node"),
        plural: "nodes".into(),
        namespaced: false,
        verbs: Default::default(),
    }
}

/// A window on `demo` (here, `node-pods-window`), with `node-a`'s detail open
/// and focused, and its pods region listing `shop/web-1` - selected, so
/// Enter/`d`/`l` have a row to act on.
fn window_with_selected_pod(cx: &mut TestAppContext) -> (Entity<MainWindow>, VisualTestContext) {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
    });
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply(
        "/api/v1",
        "nodes",
        json!({ "apiVersion": "v1", "kind": "Node", "metadata": { "name": "node-a" } }),
    );
    cluster.apply(
        "/api/v1",
        "pods",
        json!({ "apiVersion": "v1", "kind": "Pod",
            "metadata": { "name": "web-1", "namespace": "shop", "uid": "u1" },
            "spec": { "nodeName": "node-a", "containers": [{ "name": "app", "image": "nginx" }] } }),
    );
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client));
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec![CONTEXT.into()], window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);

    let node = ObjectTarget {
        kind: node_kind(),
        namespace: None,
        name: "node-a".into(),
    };
    vcx.update(|window, cx| {
        main.update(cx, |main, cx| {
            main.open_target(NavTarget::Object(node), window, cx)
        })
    });
    vcx.run_until_parked();

    let pods_region = vcx.update(|_, cx| {
        let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
            panic!("a connected window is in workspace mode")
        };
        open_panels.iter().find_map(|open| match &open.panel {
            Some(OpenedPanel::ObjectDetail(panel)) => panel.read(cx).test_node_pods(),
            _ => None,
        })
    });
    let pods_region = pods_region.expect("the Node's own pods region");

    for _ in 0..400 {
        vcx.run_until_parked();
        vcx.update(|window, cx| window.render_frame(cx));
        let has_row = vcx.update(|_, cx| !pods_region.read(cx).test_row_names(cx).is_empty());
        if has_row {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        vcx.update(|_, cx| pods_region.read(cx).test_row_names(cx)),
        ["web-1"],
        "the region lists web-1"
    );

    vcx.update(|window, cx| {
        pods_region.read(cx).focus_handle(cx).focus(window, cx);
    });
    vcx.run_until_parked();
    vcx.simulate_keystrokes("down");
    vcx.run_until_parked();
    let selected = vcx.update(|_, cx| cx.try_global::<SelectedPod>().and_then(|s| s.0.clone()));
    assert_eq!(
        selected.map(|s| s.name),
        Some("web-1".to_string()),
        "Down selected the row"
    );

    (main, vcx)
}

fn pod_detail_opened(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| {
        let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
            return false;
        };
        open_panels.iter().any(|open| {
            open.key.target
                == NavTarget::Pod(PodRef {
                    namespace: "shop".into(),
                    name: "web-1".into(),
                })
                && open.key.context_name == CONTEXT
        })
    })
}

fn logs_opened(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| {
        let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
            return false;
        };
        open_panels.iter().any(|open| {
            open.key.target
                == NavTarget::PodLogs(PodRef {
                    namespace: "shop".into(),
                    name: "web-1".into(),
                })
                && open.key.context_name == CONTEXT
        })
    })
}

/// Enter on the selected row opens its detail panel, on the node's own
/// context - the same `SelectedPod`/`ShowPodDetail` route the standalone
/// Pods panel's own `d` already takes.
#[gpui_kit::test]
async fn enter_opens_the_selected_pods_detail(cx: &mut TestAppContext) {
    let (main, mut vcx) = window_with_selected_pod(cx);
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    assert!(
        pod_detail_opened(&main, &mut vcx),
        "enter opened web-1's detail in {CONTEXT}"
    );
}

/// `d` - the embedded table's own `DescribePod` - reaches the same open.
#[gpui_kit::test]
async fn d_opens_the_selected_pods_detail(cx: &mut TestAppContext) {
    let (main, mut vcx) = window_with_selected_pod(cx);
    vcx.simulate_keystrokes("d");
    vcx.run_until_parked();
    assert!(
        pod_detail_opened(&main, &mut vcx),
        "d opened web-1's detail in {CONTEXT}"
    );
}

/// `l` opens the selected pod's logs, on the node's own context.
#[gpui_kit::test]
async fn l_opens_the_selected_pods_logs(cx: &mut TestAppContext) {
    let (main, mut vcx) = window_with_selected_pod(cx);
    vcx.simulate_keystrokes("l");
    vcx.run_until_parked();
    assert!(
        logs_opened(&main, &mut vcx),
        "l opened web-1's logs in {CONTEXT}"
    );
}
