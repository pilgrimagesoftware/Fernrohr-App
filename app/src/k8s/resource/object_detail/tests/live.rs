//! `live-detail-panels` 2.1: the panel follows its object through the shared
//! watch of its kind - or the poll, for a kind that can't be watched. A panel in
//! a window over a fake cluster (`k8s::test_cluster`) the test writes to while
//! the panel is open.

use super::fixtures::{deployments, kind, target};
use super::sections::field;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::{DiscoveredKind, KindVerbs};
use crate::k8s::cluster::session::{ClusterRegistry, WatchKey};
use crate::k8s::resource::object_detail::ObjectDetailPanel;
use crate::k8s::resource::object_detail::fetch::ObjectDetailState;
use crate::k8s::resource::pod_detail::DetailView;
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
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
        gpui_kit::init(cx);
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

    let key = WatchKey::Kind(deployments());
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

/// Design D2: an object deleted while its panel is open reads as gone, and one
/// recreated under the same name (a new uid) is shown in its place.
#[gpui_kit::test]
async fn a_deleted_then_recreated_object_shows_gone_then_the_new_one(cx: &mut TestAppContext) {
    let mut harness = open(cx, deployments(), deployment("d1", 2, 2));
    harness.wait_for("showed the object", |panel| {
        shown_uid(panel).as_deref() == Some("d1")
    });

    harness
        .cluster
        .delete(APPS, "deployments", "staging", "web");
    harness.wait_for("showed the object gone", |panel| {
        matches!(panel.state, ObjectDetailState::NotFound)
    });

    harness
        .cluster
        .apply(APPS, "deployments", deployment("d2", 1, 0));
    harness.wait_for("showed the new object", |panel| {
        shown_uid(panel).as_deref() == Some("d2")
    });
}
