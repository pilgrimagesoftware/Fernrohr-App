//! `live-detail-panels` section 1: the panel follows its pod through the
//! context's shared Pods watch. A panel in a window over a fake cluster
//! (`k8s::test_cluster`) whose pod the test changes while the panel is open.

use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::{ClusterRegistry, WatchKey};
use crate::k8s::resource::pod_detail::model::{DetailSection, DetailView};
use crate::k8s::resource::pod_detail::panel::PodDetailPanel;
use crate::k8s::resource::pod_detail::register_commands;
use crate::k8s::test_cluster::FakeCluster;
use crate::keymap::KeymapConfig;
use crate::ui::detail::lifecycle::{BANNER_ID, Lifecycle};
use crate::ui::nav::{NavTarget, PodRef};
use crate::ui::panel_title::PanelScope;
use gpui_kit::test::TestWindowExt as _;
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

/// `shop/web-1` (uid `uid`, restarted `restarts` times) being deleted: a
/// deletion timestamp `grace_secs` out.
fn terminating(uid: &str, restarts: i32, grace_secs: i64) -> Value {
    let mut pod = pod(uid, "Running", restarts);
    let deadline = jiff::Timestamp::now() + jiff::SignedDuration::from_secs(grace_secs);
    pod["metadata"]["deletionTimestamp"] = json!(deadline.to_string());
    pod["metadata"]["deletionGracePeriodSeconds"] = json!(grace_secs);
    pod
}

/// An event about `shop/web-1` (uid `uid`), seen just now.
fn event(name: &str, uid: &str, reason: &str) -> Value {
    json!({
        "apiVersion": "v1", "kind": "Event",
        "metadata": { "name": name, "namespace": "shop", "uid": format!("event-{name}") },
        "involvedObject": { "kind": "Pod", "apiVersion": "v1", "namespace": "shop", "name": "web-1", "uid": uid },
        "type": "Warning", "reason": reason, "message": format!("{reason} happened"),
        "count": 1, "lastTimestamp": jiff::Timestamp::now().to_string(),
    })
}

/// The reasons of the events the Events tab lists.
fn event_reasons(panel: &PodDetailPanel, cx: &gpui_kit::App) -> Vec<String> {
    let mut reasons: Vec<String> = panel
        .events_view(jiff::Timestamp::now(), cx)
        .and_then(|view| view.events.ok())
        .unwrap_or_default()
        .into_iter()
        .map(|event| event.reason)
        .collect();
    reasons.sort();
    reasons
}

fn banner_shown(harness: &mut Harness) -> bool {
    harness.vcx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(BANNER_ID).is_some()
    })
}

/// Spec "Deleted while open": the panel shows Terminating with the grace period
/// left, then that the pod was deleted - keeping its last fields, marked stale,
/// and its events - and stays open.
#[gpui_kit::test]
async fn a_deleted_pod_shows_terminating_then_its_last_state_as_deleted(cx: &mut TestAppContext) {
    let mut harness = open(cx, pod("u1", "Running", 4));
    harness
        .cluster
        .apply("/api/v1", "events", event("web-1.oom", "u1", "OOMKilling"));
    harness.wait_for("listed the pod's event", |panel, cx| {
        panel.live_synced(cx) && event_reasons(panel, cx) == ["OOMKilling"]
    });
    assert!(!banner_shown(&mut harness), "a live pod has no banner");

    harness
        .cluster
        .apply("/api/v1", "pods", terminating("u1", 4, 30));
    harness.wait_for("showed the pod Terminating", |panel, _| {
        matches!(panel.lifecycle(), Some(Lifecycle::Terminating { .. }))
    });
    let message = harness.vcx.update(|_, cx| {
        let lifecycle = harness.panel.read(cx).lifecycle().expect("terminating");
        lifecycle.message("pod", jiff::Timestamp::now())
    });
    assert!(
        message.starts_with("Terminating: this pod's grace period ends in"),
        "{message}"
    );
    assert!(banner_shown(&mut harness), "the banner says so");

    harness.cluster.delete("/api/v1", "pods", "shop", "web-1");
    harness.wait_for("showed the pod deleted", |panel, _| {
        matches!(panel.lifecycle(), Some(Lifecycle::Deleted { .. }))
    });
    harness.vcx.update(|_, cx| {
        let panel = harness.panel.read(cx);
        assert_eq!(
            shown(panel),
            Some(("Running".into(), 4)),
            "its last known state stays"
        );
        assert!(
            panel
                .lifecycle()
                .is_some_and(|lifecycle| lifecycle.is_stale())
        );
        assert_eq!(event_reasons(panel, cx), ["OOMKilling"], "and its events");
        let message = panel
            .lifecycle()
            .unwrap()
            .message("pod", jiff::Timestamp::now());
        assert!(message.starts_with("This pod was deleted at "), "{message}");
    });
    assert!(
        banner_shown(&mut harness),
        "the panel stays open, saying so"
    );
}

/// Spec "Recreated under the same name": the panel switches to the new pod,
/// says it replaced the deleted one, follows it live, and lists the new pod's
/// events beside the old one's.
#[gpui_kit::test]
async fn a_recreated_pod_replaces_the_deleted_one_with_a_notice(cx: &mut TestAppContext) {
    let mut harness = open(cx, pod("u1", "Running", 3));
    harness
        .cluster
        .apply("/api/v1", "events", event("web-1.old", "u1", "Killing"));
    harness.wait_for("showed the pod", |panel, cx| {
        panel.live_synced(cx) && shown_uid(panel).as_deref() == Some("u1")
    });

    harness.cluster.delete("/api/v1", "pods", "shop", "web-1");
    harness.wait_for("showed the pod deleted", |panel, _| {
        matches!(panel.lifecycle(), Some(Lifecycle::Deleted { .. }))
    });

    harness
        .cluster
        .apply("/api/v1", "pods", pod("u2", "Pending", 0));
    harness
        .cluster
        .apply("/api/v1", "events", event("web-1.new", "u2", "Scheduled"));
    harness.wait_for("showed the new pod with both pods' events", |panel, cx| {
        shown_uid(panel).as_deref() == Some("u2")
            && panel.lifecycle() == Some(Lifecycle::Replaced)
            && event_reasons(panel, cx) == ["Killing", "Scheduled"]
    });
    assert!(banner_shown(&mut harness), "the notice shows");

    harness
        .cluster
        .apply("/api/v1", "pods", pod("u2", "Running", 0));
    harness.wait_for("followed the new pod", |panel, _| {
        shown(panel) == Some(("Running".into(), 0))
    });
}
