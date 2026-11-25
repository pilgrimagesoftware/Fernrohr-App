//! #457: Controls lists a controller's pods for an account that may list pods only in the
//! controller's namespace, through a watch of that namespace, over a fake cluster that
//! refuses lists anywhere else.

use super::fixtures::{replica_sets, target};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::{ClusterRegistry, WatchKey};
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::object_detail::ObjectDetailPanel;
use crate::k8s::resource::object_detail::model::FieldValue;
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::json;

const CONTEXT: &str = "rbac-ctx";

/// A pod in `namespace` owned by the ReplicaSet with uid `rs-1`.
fn pod(name: &str, namespace: &str) -> serde_json::Value {
    json!({ "apiVersion": "v1", "kind": "Pod",
        "metadata": { "name": name, "namespace": namespace, "uid": format!("uid-{name}"),
            "ownerReferences": [{ "apiVersion": "apps/v1", "kind": "ReplicaSet",
                "name": "web", "uid": "rs-1", "controller": true }] } })
}

/// What Controls shows: its references, or the status text standing in for them.
fn controls(panel: &gpui_kit::Entity<ObjectDetailPanel>, vcx: &mut VisualTestContext) -> String {
    vcx.update(|_, cx| {
        let sections = panel.read(cx).sections(jiff::Timestamp::now());
        sections[0]
            .fields
            .iter()
            .find(|field| field.label == "Controls")
            .map(|field| match &field.value {
                FieldValue::References { targets, .. } => targets
                    .iter()
                    .map(|target: &ObjectRef| target.name.clone())
                    .collect::<Vec<_>>()
                    .join(","),
                FieldValue::Status { text, .. } => text.clone(),
                other => format!("{other:?}"),
            })
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
async fn controls_lists_pods_through_the_namespaces_own_watch(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply(
        "/apis/apps/v1",
        "replicasets",
        json!({ "apiVersion": "apps/v1", "kind": "ReplicaSet",
            "metadata": { "name": "web", "namespace": "staging", "uid": "rs-1" } }),
    );
    cluster.apply("/api/v1", "pods", pod("web-1", "staging"));
    cluster.apply("/api/v1", "pods", pod("elsewhere", "prod"));
    cluster.allow_lists_only_in(&["staging"]);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client));
    });
    let target = target(replica_sets(), Some("staging"), "web");
    let window = cx.add_window(|_, cx| {
        let scope = PanelScope::new(NavTarget::Object(target.clone()), CONTEXT.into());
        ObjectDetailPanel::new(target, scope, cx)
    });
    let panel = window.root(cx).unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);

    let mut shown = String::new();
    for _ in 0..400 {
        vcx.run_until_parked();
        shown = controls(&panel, &mut vcx);
        if shown == "web-1" {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(shown, "web-1", "the pod, not \"can't list pods\"");
    let held = |key: &WatchKey, vcx: &mut VisualTestContext| {
        vcx.update(|_, cx| ClusterRegistry::subscribers(cx, CONTEXT, key))
    };
    assert_eq!(held(&WatchKey::PodsIn("staging".into()), &mut vcx), 1);
    assert_eq!(held(&WatchKey::Pods, &mut vcx), 0, "nothing cluster-wide");
}
