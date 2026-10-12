//! `selector-logs-panel` 1.1: the typed panel's behaviour the `selector-logs`
//! spec names, beyond what the tests above it already pin - a blank selector
//! rejected, a deleted pod's stream dropped, and the stream cap.

use super::{logs_panel, pod, wait_for_streams, window};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::nav::NavTarget;
use crate::util::logs::{LabelLogs, LogsPanel};
use crate::util::shell::MainWindow;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

/// A window over a fake cluster holding `pods`, with the typed label panel
/// opened as the palette opens it, its field focused.
fn typed_panel(
    pods: Vec<serde_json::Value>,
    cx: &mut TestAppContext,
) -> (
    FakeCluster,
    Entity<MainWindow>,
    Entity<LogsPanel>,
    VisualTestContext,
) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (cluster, client) = FakeCluster::start(cx);
    for pod in pods {
        cluster.apply("/api/v1", "pods", pod);
    }
    let (main, mut vcx) = window(ConnectionState::Connected(client), cx);
    vcx.update(|window, cx| {
        let focus = main.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
        window.dispatch_action(Box::new(crate::util::logs::ShowLabelLogs), cx)
    });
    vcx.run_until_parked();
    let panel = logs_panel(&main, &NavTarget::LabelLogs(LabelLogs::Typed), &mut vcx);
    (cluster, main, panel, vcx)
}

/// "Empty selector": Enter on a blank field says a requirement is needed and
/// streams nothing.
#[gpui_kit::test]
async fn a_blank_selector_is_rejected(cx: &mut TestAppContext) {
    let (_cluster, _main, panel, mut vcx) = typed_panel(vec![pod("web-1", "web", 0)], cx);
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    let (selector, error, streams) = vcx.update(|_, cx| {
        let panel = panel.read(cx);
        (
            panel.test_selector(),
            panel.test_selector_error(),
            panel.test_streams(),
        )
    });
    assert_eq!(selector, None);
    assert!(error.is_some(), "the panel says why");
    assert!(streams.is_empty());
}

/// "A pod is deleted": its stream stops and the matched count drops.
#[gpui_kit::test]
async fn a_deleted_pods_stream_stops(cx: &mut TestAppContext) {
    let (cluster, _main, panel, mut vcx) =
        typed_panel(vec![pod("web-1", "web", 0), pod("web-2", "web", 0)], cx);
    vcx.simulate_keystrokes("a p p = w e b enter");
    wait_for_streams(
        &mut vcx,
        &panel,
        &["web-1/app#0@uid-web-1", "web-2/app#0@uid-web-2"],
    );
    assert_eq!(vcx.update(|_, cx| panel.read(cx).test_matched()), (2, 2));

    cluster.delete("/api/v1", "pods", "shop", "web-2");
    wait_for_streams(&mut vcx, &panel, &["web-1/app#0@uid-web-1"]);
    assert_eq!(vcx.update(|_, cx| panel.read(cx).test_matched()), (1, 1));
}

/// "More matches than the cap": 25 matching containers, 20 streaming, in the
/// stable order - the first 20 by namespace and name.
#[gpui_kit::test]
async fn only_the_cap_streams(cx: &mut TestAppContext) {
    let pods = (0..25)
        .map(|n| pod(&format!("web-{n:02}"), "web", 0))
        .collect();
    let (_cluster, _main, panel, mut vcx) = typed_panel(pods, cx);
    vcx.simulate_keystrokes("a p p = w e b enter");
    let expected: Vec<String> = (0..20)
        .map(|n| format!("web-{n:02}/app#0@uid-web-{n:02}"))
        .collect();
    let expected: Vec<&str> = expected.iter().map(String::as_str).collect();
    wait_for_streams(&mut vcx, &panel, &expected);
    assert_eq!(vcx.update(|_, cx| panel.read(cx).test_matched()), (25, 25));
}

/// `selector-logs-panel` 2.1: the Window menu's Open Logs by Label… is the
/// palette's command, and dispatching it - as the menu does - opens the typed
/// panel empty, its field focused.
#[gpui_kit::test]
async fn the_menu_item_opens_the_panel_empty(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (_cluster, client) = FakeCluster::start(cx);
    let (main, mut vcx) = window(ConnectionState::Connected(client), cx);
    let mut registry = crate::command::CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    let command = registry
        .get("logs.follow_labels")
        .expect("Open Logs by Label is registered");
    assert_eq!(command.title, "Open Logs by Label\u{2026}");
    assert_eq!(command.menu, Some(crate::command::MenuSlot::Window));

    let action = command.action.boxed_clone();
    vcx.update(|window, cx| {
        let focus = main.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
        window.dispatch_action(action, cx)
    });
    vcx.run_until_parked();
    let panel = logs_panel(&main, &NavTarget::LabelLogs(LabelLogs::Typed), &mut vcx);
    assert_eq!(vcx.update(|_, cx| panel.read(cx).test_selector()), None);
    assert!(vcx.update(|_, cx| panel.read(cx).test_streams()).is_empty());
    // The field has focus: what's typed lands in it.
    vcx.simulate_keystrokes("a p p = w e b enter");
    vcx.run_until_parked();
    assert_eq!(
        vcx.update(|_, cx| panel.read(cx).test_selector()),
        Some("app=web".to_string())
    );
}

/// A `shop` pod labelled `app=web` running `app` and `sidecar`.
fn two_container_pod(name: &str) -> serde_json::Value {
    let status = |container: &str| {
        serde_json::json!({
            "name": container, "ready": true, "restartCount": 0, "image": container,
            "imageID": "", "state": { "running": {} },
        })
    };
    serde_json::json!({
        "apiVersion": "v1", "kind": "Pod",
        "metadata": {
            "name": name, "namespace": "shop", "uid": format!("uid-{name}"),
            "labels": { "app": "web" },
        },
        "status": {
            "phase": "Running",
            "containerStatuses": [status("app"), status("sidecar")],
        },
    })
}

/// `selector-logs-panel` 4.1-4.2 by keyboard: `c` steps the container filter
/// through each name and back to all, the streams following - a container
/// filtered out is dropped - and the panel saves the filter it shows.
#[gpui_kit::test]
async fn c_steps_the_container_filter(cx: &mut TestAppContext) {
    let (_cluster, _main, panel, mut vcx) = typed_panel(vec![two_container_pod("web-1")], cx);
    vcx.simulate_keystrokes("a p p = w e b enter");
    let both = ["web-1/app#0@uid-web-1", "web-1/sidecar#0@uid-web-1"];
    wait_for_streams(&mut vcx, &panel, &both);
    // Out of the selector field, where `c` would type.
    vcx.update(|window, cx| {
        use gpui_kit::Focusable as _;
        let handle = panel.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    });
    let saved_filter = |vcx: &mut VisualTestContext| {
        vcx.update(|_, cx| {
            use gpui_kit::component::dock::{BasePanel as _, PanelInfo};
            let PanelInfo::Panel(info) = panel.read(cx).dump(cx).info else {
                panic!("a panel's state");
            };
            crate::util::logs::labels_from_state(&info)
                .expect("a label panel")
                .container
        })
    };

    vcx.simulate_keystrokes("c");
    wait_for_streams(&mut vcx, &panel, &["web-1/app#0@uid-web-1"]);
    assert_eq!(saved_filter(&mut vcx).as_deref(), Some("app"));

    vcx.simulate_keystrokes("c");
    wait_for_streams(&mut vcx, &panel, &["web-1/sidecar#0@uid-web-1"]);
    assert_eq!(vcx.update(|_, cx| panel.read(cx).test_matched()), (1, 1));

    vcx.simulate_keystrokes("c");
    wait_for_streams(&mut vcx, &panel, &both);
    assert_eq!(
        saved_filter(&mut vcx),
        None,
        "all containers saves no filter"
    );
}
