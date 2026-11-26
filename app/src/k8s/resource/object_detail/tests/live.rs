//! `live-detail-panels` 2.1: the panel follows its object through the shared
//! watch of its kind - or the poll, for a kind that can't be watched. A panel in
//! a window over a fake cluster (`k8s::test_cluster`) the test writes to while
//! the panel is open.

use super::fixtures::{deployments, kind, target};
use super::sections::field;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::{DiscoveredKind, KindVerbs};
use crate::k8s::cluster::session::{ClusterRegistry, KindKey, WatchKey};
use crate::k8s::resource::object_detail::ObjectDetailPanel;
use crate::k8s::resource::pod_detail::DetailView;
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::detail::lifecycle::{BANNER_ID, Lifecycle};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};

const CONTEXT: &str = "kind-dev";
const APPS: &str = "/apis/apps/v1";

/// `staging/web` with uid `uid`, wanting `desired` replicas of which `ready` are ready and available.
fn deployment(uid: &str, desired: i64, ready: i64) -> Value {
    json!({
        "apiVersion": "apps/v1", "kind": "Deployment",
        "metadata": { "name": "web", "namespace": "staging", "uid": uid },
        "spec": { "replicas": desired, "selector": { "matchLabels": { "app": "web" } }, "template": {} },
        "status": { "updatedReplicas": desired, "readyReplicas": ready, "availableReplicas": ready },
    })
}

struct Harness {
    cluster: FakeCluster,
    panel: Entity<ObjectDetailPanel>,
    vcx: VisualTestContext,
}

/// The panel open on `staging/web` of `kind`, which the fake cluster holds as `initial`.
fn open(cx: &mut TestAppContext, kind: DiscoveredKind, initial: Value) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply(APPS, "deployments", initial);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client));
    });
    let target = target(kind, Some("staging"), "web");
    let window = cx.add_window(|_, cx| {
        let scope = PanelScope::new(NavTarget::Object(target.clone()), CONTEXT.into());
        ObjectDetailPanel::new(target, scope, cx)
    });
    let panel = window.root(cx).unwrap();
    let vcx = VisualTestContext::from_window(window.into(), cx);
    Harness {
        cluster,
        panel,
        vcx,
    }
}

impl Harness {
    /// Runs the app until `done` holds for the panel, panicking with `what` if it never does.
    fn wait_for(&mut self, what: &str, done: impl Fn(&ObjectDetailPanel) -> bool) {
        for _ in 0..400 {
            self.vcx.run_until_parked();
            if self.vcx.update(|_, cx| done(self.panel.read(cx))) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("never {what}");
    }
}

/// The Replicas row as the panel shows it, once the object is in.
fn replicas(panel: &ObjectDetailPanel) -> Option<String> {
    panel.object()?;
    Some(
        field(&panel.sections(jiff::Timestamp::now()), "Replicas")
            .value
            .text(),
    )
}

fn shown_uid(panel: &ObjectDetailPanel) -> Option<String> {
    panel.object()?.metadata.uid.clone()
}

/// Spec "A Deployment rolls out": scaled from 2 to 3 while its panel is open,
/// the counts follow the rollout without a reopen, on the view the user chose -
/// and closing the panel releases its share of the kind's watch.
#[gpui_kit::test]
async fn a_deployments_replica_counts_follow_its_rollout(cx: &mut TestAppContext) {
    let mut harness = open(cx, deployments(), deployment("d1", 2, 2));
    harness.wait_for("showed 2 replicas", |panel| {
        replicas(panel).as_deref() == Some("desired 2 · updated 2 · ready 2 · available 2")
    });
    harness.vcx.update(|_, cx| {
        harness
            .panel
            .update(cx, |panel, cx| panel.set_view(DetailView::Yaml, cx));
    });

    harness
        .cluster
        .apply(APPS, "deployments", deployment("d1", 3, 2));
    harness.wait_for("showed the scale-up", |panel| {
        replicas(panel).as_deref() == Some("desired 3 · updated 3 · ready 2 · available 2")
    });
    harness
        .cluster
        .apply(APPS, "deployments", deployment("d1", 3, 3));
    harness.wait_for("showed the rollout finish", |panel| {
        replicas(panel).as_deref() == Some("desired 3 · updated 3 · ready 3 · available 3")
    });
    let view = harness.vcx.update(|_, cx| harness.panel.read(cx).view());
    assert_eq!(view, DetailView::Yaml, "the view stayed on the YAML");

    // The panel watches only its object's namespace, since no list holds the cluster-wide watch.
    let key = WatchKey::Kind(KindKey {
        kind: deployments(),
        namespace: Some("staging".to_string()),
    });
    let subscribers = harness
        .vcx
        .update(|_, cx| ClusterRegistry::subscribers(cx, CONTEXT, &key));
    assert_eq!(subscribers, 1, "the panel shares the kind's watch");
    let Harness { panel, mut vcx, .. } = harness;
    drop(panel);
    vcx.update(|window, _| window.remove_window());
    cx.run_until_parked();
    let subscribers = cx.update(|cx| ClusterRegistry::subscribers(cx, CONTEXT, &key));
    assert_eq!(subscribers, 0, "closing the panel released the watch");
}

/// A kind that can be listed but not watched is polled; the panel follows the
/// poll's next list as it follows a watch.
#[gpui_kit::test]
async fn a_polled_kinds_object_updates_on_the_next_list(cx: &mut TestAppContext) {
    let polled = DiscoveredKind {
        verbs: KindVerbs {
            list: true,
            watch: false,
            delete: true,
            patch: true,
        },
        ..kind("apps", "v1", "Deployment", true)
    };
    let mut harness = open(cx, polled, deployment("d1", 2, 2));
    harness.wait_for("showed 2 replicas", |panel| {
        replicas(panel).as_deref() == Some("desired 2 · updated 2 · ready 2 · available 2")
    });

    harness
        .cluster
        .apply(APPS, "deployments", deployment("d1", 3, 2));
    harness.vcx.update(|_, cx| {
        let table = harness.panel.read(cx).live_table().expect("following");
        table.read(cx).request_refresh();
    });
    harness.wait_for("showed the scale-up", |panel| {
        replicas(panel).as_deref() == Some("desired 3 · updated 3 · ready 2 · available 2")
    });
}

/// Spec "Deleted while open": the panel says the object was deleted and when,
/// keeps its last state visible as stale, and stays open; one recreated under
/// the same name (a new uid) is then shown in its place, saying so.
#[gpui_kit::test]
async fn a_deleted_object_keeps_its_last_state_until_replaced(cx: &mut TestAppContext) {
    let mut harness = open(cx, deployments(), deployment("d1", 2, 2));
    harness.wait_for("showed the object", |panel| {
        shown_uid(panel).as_deref() == Some("d1")
    });

    harness
        .cluster
        .delete(APPS, "deployments", "staging", "web");
    harness.wait_for("showed the object deleted", |panel| {
        matches!(panel.lifecycle, Some(Lifecycle::Deleted { .. }))
    });
    harness.vcx.update(|window, cx| {
        let panel = harness.panel.read(cx);
        assert_eq!(
            replicas(panel).as_deref(),
            Some("desired 2 · updated 2 · ready 2 · available 2"),
            "its last known state stays"
        );
        let message = panel
            .lifecycle
            .as_ref()
            .unwrap()
            .message("deployment", jiff::Timestamp::now());
        assert!(
            message.starts_with("This deployment was deleted at "),
            "{message}"
        );
        window.render_frame(cx);
        assert!(window.try_find(BANNER_ID).is_some(), "the panel says so");
    });

    harness
        .cluster
        .apply(APPS, "deployments", deployment("d2", 1, 0));
    harness.wait_for("showed the new object", |panel| {
        shown_uid(panel).as_deref() == Some("d2") && panel.lifecycle == Some(Lifecycle::Replaced)
    });
}

/// #162: a fetch sent just before the delete can land after the table's
/// `DELETED`. What it found is stale - the panel keeps the "deleted" notice
/// rather than showing the object as current with nothing left to correct it.
#[gpui_kit::test]
async fn a_fetch_landing_after_the_delete_keeps_the_object_deleted(cx: &mut TestAppContext) {
    use crate::k8s::resource::object_detail::fetch::ObjectFetch;

    let mut harness = open(cx, deployments(), deployment("d1", 2, 2));
    harness.wait_for("showed the object", |panel| {
        shown_uid(panel).as_deref() == Some("d1")
    });
    harness
        .cluster
        .delete(APPS, "deployments", "staging", "web");
    harness.wait_for("showed the object deleted", |panel| {
        matches!(panel.lifecycle, Some(Lifecycle::Deleted { .. }))
    });

    // The stale fetch lands: it found the object as it was before the delete.
    let stale: kube::api::DynamicObject =
        serde_json::from_value(deployment("d1", 2, 2)).expect("a Deployment");
    harness.vcx.update(|_, cx| {
        harness.panel.update(cx, |panel, cx| {
            panel.land_fetch(Ok(ObjectFetch::Found(Box::new(stale), Ok(Vec::new()))), cx)
        })
    });
    harness.vcx.run_until_parked();

    harness.vcx.update(|_, cx| {
        let panel = harness.panel.read(cx);
        assert!(
            matches!(panel.lifecycle, Some(Lifecycle::Deleted { .. })),
            "still shown deleted"
        );
        assert_eq!(
            shown_uid(panel).as_deref(),
            Some("d1"),
            "its last state stays"
        );
    });
}

/// #457: a panel that shared a list's cluster-wide watch moves to its object's namespace
/// when the server starts refusing the cluster-wide list - say a role narrowed to that
/// namespace - instead of showing "can't list".
#[gpui_kit::test]
async fn a_refused_shared_watch_moves_the_panel_to_its_namespace(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply(APPS, "deployments", deployment("uid-1", 2, 2));
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            CONTEXT,
            ConnectionState::Connected(client.clone()),
        );
    });
    // A list holds the cluster-wide watch, so the panel shares it.
    let list = cx.update(|cx| ClusterRegistry::subscribe_kind(cx, CONTEXT, client, &deployments()));
    let target = target(deployments(), Some("staging"), "web");
    let window = cx.add_window(|_, cx| {
        let scope = PanelScope::new(NavTarget::Object(target.clone()), CONTEXT.into());
        ObjectDetailPanel::new(target, scope, cx)
    });
    let panel = window.root(cx).unwrap();
    let mut harness = Harness {
        cluster,
        panel,
        vcx: VisualTestContext::from_window(window.into(), cx),
    };
    harness.wait_for("showed the replicas", |panel| replicas(panel).is_some());
    let all = WatchKey::Kind(KindKey::all(&deployments()));
    let staging = WatchKey::Kind(KindKey {
        kind: deployments(),
        namespace: Some("staging".to_string()),
    });
    let count = |harness: &mut Harness, key: &WatchKey| {
        harness
            .vcx
            .update(|_, cx| ClusterRegistry::subscribers(cx, CONTEXT, key))
    };
    assert_eq!(count(&mut harness, &all), 2, "the list and the panel");

    // The cluster-wide list is refused from now on; a reconnect relists it.
    harness.cluster.allow_lists_only_in(&["staging"]);
    harness.vcx.update(|_, cx| {
        use crate::k8s::cluster::health::HealthTransition;
        use crate::k8s::cluster::watch_registry::PauseReason;
        ClusterRegistry::apply_health_transition(
            cx,
            CONTEXT,
            HealthTransition::Pause(PauseReason::Reconnecting),
        );
        ClusterRegistry::apply_health_transition(cx, CONTEXT, HealthTransition::Resume);
    });
    let mut refused = false;
    for _ in 0..400 {
        harness.vcx.run_until_parked();
        refused = harness
            .vcx
            .update(|_, cx| list.read(cx).refused().is_some());
        if refused && count(&mut harness, &staging) == 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(refused, "the cluster-wide list was refused");
    assert_eq!(
        count(&mut harness, &staging),
        1,
        "the panel moved to staging"
    );
    assert_eq!(count(&mut harness, &all), 1, "only the list holds it now");

    // Still live, through the namespace's watch.
    harness
        .cluster
        .apply(APPS, "deployments", deployment("uid-1", 3, 3));
    harness.wait_for("followed the next write", |panel| {
        replicas(panel).as_deref() == Some("desired 3 · updated 3 · ready 3 · available 3")
    });
    cx.update(|cx| ClusterRegistry::unsubscribe_kind(cx, CONTEXT, &deployments()));
}
