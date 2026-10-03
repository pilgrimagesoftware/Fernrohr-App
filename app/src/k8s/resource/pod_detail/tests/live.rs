//! `live-detail-panels` section 1: the panel follows its pod through the
//! context's shared Pods watch. A panel in a window over a fake cluster
//! (`k8s::test_cluster`) whose pod the test changes while the panel is open.

use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::{ClusterRegistry, WatchKey};
use crate::k8s::resource::pod_detail::fetch::PodDetailState;
use crate::k8s::resource::pod_detail::model::{DetailSection, DetailView};
use crate::k8s::resource::pod_detail::panel::PodDetailPanel;
use crate::k8s::resource::pod_detail::register_commands;
use crate::k8s::test_cluster::FakeCluster;
use crate::keymap::KeymapConfig;
use crate::ui::nav::{NavTarget, PodRef};
use crate::ui::panel_title::PanelScope;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};

const CONTEXT: &str = "kind-dev";

/// `shop/web-1` with uid `uid`, in `phase`, its one container restarted `restarts` times.
fn pod(uid: &str, phase: &str, restarts: i32) -> Value {
    json!({
        "apiVersion": "v1", "kind": "Pod",
        "metadata": { "name": "web-1", "namespace": "shop", "uid": uid },
        "spec": { "containers": [{ "name": "app", "image": "nginx" }] },
        "status": {
            "phase": phase,
            "containerStatuses": [{
                "name": "app", "image": "nginx", "imageID": "",
                "ready": phase == "Running", "restartCount": restarts,
            }],
        },
    })
}

struct Harness {
    cluster: FakeCluster,
    panel: Entity<PodDetailPanel>,
    vcx: VisualTestContext,
}

/// The panel open on `shop/web-1`, which the fake cluster holds as `initial`,
/// with the panel's keys bound and focus in it.
fn open(cx: &mut TestAppContext, initial: Value) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        register_commands(&mut registry);
        let bindings = crate::keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
    });
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply("/api/v1", "pods", initial);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client));
    });
    let window = cx.add_window(|_, cx| {
        let pod = PodRef {
            namespace: "shop".into(),
            name: "web-1".into(),
        };
        let scope = PanelScope::new(NavTarget::pod("shop", "web-1"), CONTEXT.into());
        PodDetailPanel::new(pod, scope, DetailView::Structured, cx)
    });
    let panel = window.root(cx).unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| {
        window.activate_window();
        panel.read(cx).focus_handle.clone().focus(window, cx);
    });
    Harness {
        cluster,
        panel,
        vcx,
    }
}

impl Harness {
    /// Runs the app until `done` holds for the panel, panicking with `what` if it never does.
    fn wait_for(&mut self, what: &str, done: impl Fn(&PodDetailPanel, &gpui_kit::App) -> bool) {
        for _ in 0..400 {
            self.vcx.run_until_parked();
            if self.vcx.update(|_, cx| done(self.panel.read(cx), cx)) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("never {what}");
    }
}

/// The shown pod's phase and its container's restart count, if one is shown.
fn shown(panel: &PodDetailPanel) -> Option<(String, i32)> {
    let pod = panel.pod()?;
    let status = pod.status.as_ref()?;
    let restarts = status.container_statuses.as_ref()?.first()?.restart_count;
    Some((status.phase.clone()?, restarts))
}

fn shown_uid(panel: &PodDetailPanel) -> Option<String> {
    panel.pod()?.metadata.uid.clone()
}

/// Spec "A pod recovers from a missing Secret": the open panel shows the pod
/// Running once the cluster says so - read off the shared watch, not refetched -
/// and closing the panel releases its share of that watch.
#[gpui_kit::test]
async fn a_pending_pod_turns_running_in_the_open_panel(cx: &mut TestAppContext) {
    let mut harness = open(cx, pod("u1", "Pending", 0));
    harness.wait_for("showed the pod Pending", |panel, cx| {
        panel.live_synced(cx) && shown(panel) == Some(("Pending".into(), 0))
    });
    let subscribers = harness
        .vcx
        .update(|_, cx| ClusterRegistry::subscribers(cx, CONTEXT, &WatchKey::Pods));
    assert_eq!(subscribers, 1, "the panel shares the context's Pods watch");

    harness
        .cluster
        .apply("/api/v1", "pods", pod("u1", "Running", 0));
    harness.wait_for("showed the pod Running", |panel, _| {
        shown(panel) == Some(("Running".into(), 0))
    });
    assert!(
        harness.cluster.gets() <= 1,
        "the change came off the watch, not a refetch ({} gets)",
        harness.cluster.gets()
    );

    let Harness { panel, mut vcx, .. } = harness;
    drop(panel);
    vcx.update(|window, _| window.remove_window());
    cx.run_until_parked();
    let subscribers = cx.update(|cx| ClusterRegistry::subscribers(cx, CONTEXT, &WatchKey::Pods));
    assert_eq!(subscribers, 0, "closing the panel released the watch");
}

/// Spec "A container restarts": the count updates in place, and the tab the
/// user switched to from the keyboard and the card they expanded stay as they were.
#[gpui_kit::test]
async fn a_restart_keeps_the_open_tab_and_an_expanded_card(cx: &mut TestAppContext) {
    let mut harness = open(cx, pod("u1", "Running", 0));
    harness.wait_for("showed the pod", |panel, cx| {
        panel.live_synced(cx) && shown(panel).is_some()
    });
    harness.vcx.simulate_keystrokes("2");
    harness.vcx.update(|_, cx| {
        harness.panel.update(cx, |panel, cx| {
            panel.open_sections.insert("container:app".into());
            cx.notify();
        })
    });

    harness
        .cluster
        .apply("/api/v1", "pods", pod("u1", "Running", 1));
    harness.wait_for("showed the restart", |panel, _| {
        shown(panel) == Some(("Running".into(), 1))
    });
    harness.vcx.update(|_, cx| {
        let panel = harness.panel.read(cx);
        assert_eq!(
            panel.active_tab(),
            DetailSection::Containers,
            "still on the tab"
        );
        assert!(
            panel.open_sections.contains("container:app"),
            "the card is still expanded"
        );
    });
}

/// Design D2: a pod deleted while its panel is open reads as gone, and one
/// recreated under the same name (a new uid) is shown in its place.
#[gpui_kit::test]
async fn a_deleted_then_recreated_pod_shows_gone_then_the_new_one(cx: &mut TestAppContext) {
    let mut harness = open(cx, pod("u1", "Running", 3));
    harness.wait_for("showed the pod", |panel, cx| {
        panel.live_synced(cx) && shown_uid(panel).as_deref() == Some("u1")
    });

    harness.cluster.delete("/api/v1", "pods", "shop", "web-1");
    harness.wait_for("showed the pod gone", |panel, _| {
        matches!(panel.state, PodDetailState::NotFound)
    });

    harness
        .cluster
        .apply("/api/v1", "pods", pod("u2", "Pending", 0));
    harness.wait_for("showed the new pod", |panel, _| {
        shown_uid(panel).as_deref() == Some("u2") && shown(panel) == Some(("Pending".into(), 0))
    });
}
