//! #150 through a real window: `l` in a Deployment's detail panel opens a Logs
//! panel following its pods, and Follow Labels opens one whose selector is
//! typed into it - each streaming every started container its selector picks.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::nav::{NavTarget, ObjectTarget, OpenedPanel};
use crate::util::logs::{LabelLogs, LogsPanel, WorkloadRef};
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, WindowMode, init};
use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use serde_json::json;

/// A workspace window over context `demo`, in `state`. The runtime is the
/// caller's to start first: a fake cluster needs it before the window does.
fn window(
    state: ConnectionState,
    cx: &mut TestAppContext,
) -> (Entity<MainWindow>, VisualTestContext) {
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        init(cx, workspace, &keymap);
        ClusterRegistry::insert_test_session(cx, "demo", state);
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec!["demo".into()], window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    (main, VisualTestContext::from_window(window.into(), cx))
}

/// The open label Logs panels' targets, in opening order.
fn label_logs(main: &Entity<MainWindow>, vcx: &mut VisualTestContext) -> Vec<NavTarget> {
    vcx.update(|_, cx| {
        let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
            return Vec::new();
        };
        open_panels
            .iter()
            .map(|open| open.key.target.clone())
            .filter(|target| matches!(target, NavTarget::LabelLogs(_)))
            .collect()
    })
}

/// The Logs panel open on `target`.
fn logs_panel(
    main: &Entity<MainWindow>,
    target: &NavTarget,
    vcx: &mut VisualTestContext,
) -> Entity<LogsPanel> {
    vcx.update(|_, cx| {
        let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
            return None;
        };
        open_panels
            .iter()
            .filter(|open| &open.key.target == target)
            .find_map(|open| match &open.panel {
                Some(OpenedPanel::Logs(panel)) => Some(panel.clone()),
                _ => None,
            })
    })
    .expect("the label Logs panel is open")
}

/// Waits - the fake cluster answers on the runtime, not the test's executor -
/// until `panel` streams exactly `expected`.
fn wait_for_streams(vcx: &mut VisualTestContext, panel: &Entity<LogsPanel>, expected: &[&str]) {
    for _ in 0..400 {
        vcx.run_until_parked();
        if vcx.update(|_, cx| panel.read(cx).test_streams()) == expected {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let streams = vcx.update(|_, cx| panel.read(cx).test_streams());
    panic!("streaming {streams:?}, not {expected:?}");
}

fn deployments() -> DiscoveredKind {
    DiscoveredKind {
        gvk: kube::core::GroupVersionKind::gvk("apps", "v1", "Deployment"),
        plural: "deployments".into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

#[gpui_kit::test]
async fn l_in_a_deployments_detail_panel_follows_its_pods(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (main, mut vcx) = window(ConnectionState::Connecting, cx);
    let deployment = NavTarget::Object(ObjectTarget {
        kind: deployments(),
        namespace: Some("shop".into()),
        name: "web".into(),
    });
    vcx.update(|window, cx| {
        main.update(cx, |main, cx| {
            main.open_target(deployment.clone(), window, cx)
        })
    });
    vcx.run_until_parked();
    let detail = vcx
        .update(|_, cx| {
            let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
                return None;
            };
            open_panels.iter().find_map(|open| match &open.panel {
                Some(OpenedPanel::ObjectDetail(panel)) => Some(panel.clone()),
                _ => None,
            })
        })
        .expect("the Deployment's detail panel");
    vcx.update(|_, cx| {
        detail.update(cx, |panel, cx| {
            let loaded = serde_json::from_value(json!({
                "apiVersion": "apps/v1", "kind": "Deployment",
                "metadata": { "name": "web", "namespace": "shop" },
                "spec": { "selector": { "matchLabels": { "app": "web" } } },
            }))
            .expect("a deployment");
            panel.test_set_loaded(loaded, cx);
        })
    });
    let focus_detail = |vcx: &mut VisualTestContext| {
        vcx.update(|window, cx| {
            use gpui_kit::Focusable as _;
            main.update(cx, |main, cx| {
                main.open_target(deployment.clone(), window, cx)
            });
            let handle = detail.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        });
        vcx.run_until_parked();
    };
    focus_detail(&mut vcx);

    vcx.simulate_keystrokes("l");
    vcx.run_until_parked();

    let followed = NavTarget::LabelLogs(LabelLogs::Workload(WorkloadRef {
        kind: "Deployment".into(),
        namespace: "shop".into(),
        name: "web".into(),
        selector: "app=web".into(),
    }));
    assert_eq!(label_logs(&main, &mut vcx), std::slice::from_ref(&followed));

    focus_detail(&mut vcx);
    vcx.simulate_keystrokes("l");
    vcx.run_until_parked();
    assert_eq!(
        label_logs(&main, &mut vcx),
        [followed],
        "the Deployment's Logs panel, focused rather than a second"
    );
}

fn pod(name: &str, app: &str, restarts: i32) -> serde_json::Value {
    json!({
        "apiVersion": "v1", "kind": "Pod",
        "metadata": {
            "name": name, "namespace": "shop", "uid": format!("uid-{name}"),
            "labels": { "app": app },
        },
        "status": {
            "phase": "Running",
            "containerStatuses": [{
                "name": "app", "ready": true, "restartCount": restarts, "image": "app",
                "imageID": "", "state": { "running": {} },
            }],
        },
    })
}

#[gpui_kit::test]
async fn a_selector_typed_into_follow_labels_streams_the_pods_it_picks(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (cluster, client) = crate::k8s::test_cluster::FakeCluster::start(cx);
    cluster.apply("/api/v1", "pods", pod("web-1", "web", 0));
    cluster.apply("/api/v1", "pods", pod("db-1", "db", 0));
    let (main, mut vcx) = window(ConnectionState::Connected(client), cx);

    // The palette's Follow Labels, dispatched as the palette does.
    vcx.update(|window, cx| {
        let focus = main.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
        window.dispatch_action(Box::new(crate::util::logs::ShowLabelLogs), cx)
    });
    vcx.run_until_parked();
    let typed = NavTarget::LabelLogs(LabelLogs::Typed);
    let panel = logs_panel(&main, &typed, &mut vcx);

    // The new panel's selector field has focus: type into it, then Enter.
    vcx.simulate_keystrokes("a p p = w e b enter");
    vcx.run_until_parked();
    assert_eq!(
        vcx.update(|_, cx| panel.read(cx).test_selector()),
        Some("app=web".to_string())
    );
    wait_for_streams(&mut vcx, &panel, &["web-1/app#0"]);

    // A pod that starts matching streams too, and a restart is a new stream.
    cluster.apply("/api/v1", "pods", pod("web-2", "web", 0));
    cluster.apply("/api/v1", "pods", pod("web-1", "web", 1));
    vcx.run_until_parked();
    wait_for_streams(&mut vcx, &panel, &["web-1/app#1", "web-2/app#0"]);

    // A selector that doesn't parse says why and keeps the last one.
    vcx.simulate_keystrokes("secondary-a backspace t e a m space i n space ( enter");
    vcx.run_until_parked();
    let (error, streams) = vcx.update(|_, cx| {
        let panel = panel.read(cx);
        (panel.test_selector_error(), panel.test_streams())
    });
    assert!(
        error
            .as_deref()
            .is_some_and(|error| error.contains("team in (")),
        "{error:?}"
    );
    assert_eq!(streams, ["web-1/app#1", "web-2/app#0"]);

    // Follow Labels again focuses the open panel, and its field.
    vcx.update(|window, cx| {
        use gpui_kit::Focusable as _;
        let handle = panel.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
        window.dispatch_action(Box::new(crate::util::logs::ShowLabelLogs), cx)
    });
    vcx.run_until_parked();
    assert_eq!(label_logs(&main, &mut vcx), [typed]);
    vcx.simulate_keystrokes("secondary-a backspace a p p = d b enter");
    vcx.run_until_parked();
    wait_for_streams(&mut vcx, &panel, &["db-1/app#0"]);
}
