//! Which entries apply to a resource, and what a new entry from one starts
//! with (`exec-command-library` D3, D4) - against objects cached by watches of
//! a fake cluster, as an open list would have them.

use super::{Resource, applies, prefill};
use crate::config::exec_library::{Entry, Target, WorkloadKind};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::test_cluster::FakeCluster;
use gpui_kit::TestAppContext;
use k8s_openapi::api::core::v1::Pod;
use kube::core::GroupVersionKind;
use serde_json::{Value, json};

const CONTEXT: &str = "demo";

fn entry(name: &str, target: Target) -> Entry {
    Entry {
        name: name.into(),
        command: "date".into(),
        container: None,
        target,
    }
}

fn selector(namespace: Option<&str>, selector: &str) -> Target {
    Target::Selector {
        namespace: namespace.map(str::to_string),
        selector: selector.into(),
    }
}

fn web_deployment() -> Target {
    Target::Workload {
        namespace: "shop".into(),
        workload_kind: WorkloadKind::Deployment,
        name: "web".into(),
    }
}

fn pod_json(labels: Value, owner: Option<(&str, &str)>) -> Value {
    let mut pod = json!({ "apiVersion": "v1", "kind": "Pod",
        "metadata": { "name": "web-7d9-abc", "namespace": "shop", "uid": "pod-1",
            "labels": labels },
        "spec": { "containers": [{ "name": "app" }] },
        "status": { "phase": "Running" } });
    if let Some((kind, name)) = owner {
        pod["metadata"]["ownerReferences"] = json!([{ "apiVersion": "apps/v1", "kind": kind,
            "name": name, "uid": format!("uid-{name}"), "controller": true }]);
    }
    pod
}

fn pod(labels: Value, owner: Option<(&str, &str)>) -> Pod {
    serde_json::from_value(pod_json(labels, owner)).unwrap()
}

fn apps(kind: &str, plural: &str) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("apps", "v1", kind),
        plural: plural.into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

/// A connected `demo` over a fake cluster holding a `web` Deployment
/// (selecting `app=web,tier=front`) and its ReplicaSet, with `kinds` watched -
/// cached, as an open list would have them.
fn cluster(cx: &mut TestAppContext, kinds: &[DiscoveredKind]) -> FakeCluster {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply(
        "/apis/apps/v1",
        "deployments",
        json!({ "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": { "name": "web", "namespace": "shop", "uid": "uid-web" },
            "spec": { "selector": { "matchLabels": { "app": "web", "tier": "front" } } } }),
    );
    cluster.apply(
        "/apis/apps/v1",
        "replicasets",
        json!({ "apiVersion": "apps/v1", "kind": "ReplicaSet",
            "metadata": { "name": "web-7d9", "namespace": "shop", "uid": "uid-web-7d9",
                "ownerReferences": [{ "apiVersion": "apps/v1", "kind": "Deployment",
                    "name": "web", "uid": "uid-web", "controller": true }] },
            "spec": { "selector": { "matchLabels": {
                "app": "web", "tier": "front", "pod-template-hash": "7d9" } } } }),
    );
    cluster.apply(
        "/api/v1",
        "pods",
        pod_json(json!({ "app": "web", "tier": "front" }), None),
    );
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            CONTEXT,
            ConnectionState::Connected(client.clone()),
        );
        for kind in kinds {
            ClusterRegistry::subscribe_kind(cx, CONTEXT, client.clone(), kind);
        }
        ClusterRegistry::subscribe_pods(cx, CONTEXT, client.clone());
    });
    for _ in 0..400 {
        cx.run_until_parked();
        let loaded = cx.update(|cx| {
            kinds.iter().all(|kind| {
                ClusterRegistry::existing_kind(cx, CONTEXT, kind)
                    .is_some_and(|table| !table.read(cx).rows().is_empty())
            }) && ClusterRegistry::existing_pods(cx, CONTEXT)
                .is_some_and(|pods| !pods.read(cx).pods().is_empty())
        });
        if loaded {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    cluster
}

/// Spec "Only matching entries offered".
#[gpui_kit::test]
async fn a_pod_is_offered_only_the_entries_that_match_it(cx: &mut TestAppContext) {
    let _cluster = cluster(cx, &[]);
    let web = Resource::Pod(Box::new(pod(json!({ "app": "web" }), None)));
    let entries = [
        entry("Anywhere", Target::Any),
        entry("Web", selector(None, "app=web")),
        entry("Web in shop", selector(Some("shop"), "app=web")),
        entry("Web elsewhere", selector(Some("staging"), "app=web")),
        entry("Db", selector(None, "app=db")),
    ];
    let offered: Vec<String> = cx.update(|cx| {
        entries
            .iter()
            .filter(|entry| applies(entry, &web, CONTEXT, cx))
            .map(|entry| entry.name.clone())
            .collect()
    });
    assert_eq!(offered, ["Anywhere", "Web", "Web in shop"]);
}

/// Spec "Workload entry on its pod": offered once the Deployment is cached -
/// and not before, since nothing is fetched to decide.
#[gpui_kit::test]
async fn a_workload_entry_applies_to_its_pod_when_the_workload_is_cached(cx: &mut TestAppContext) {
    let web = Resource::Pod(Box::new(pod(
        json!({ "app": "web", "tier": "front" }),
        None,
    )));
    let deployment = entry("Web deployment", web_deployment());

    let _cluster = cluster(cx, &[]);
    assert!(
        !cx.update(|cx| applies(&deployment, &web, CONTEXT, cx)),
        "not cached"
    );

    cx.update(|cx| {
        let client = match ClusterRegistry::existing_connection(cx, CONTEXT)
            .expect("connected")
            .read(cx)
            .state
            .clone()
        {
            ConnectionState::Connected(client) => client,
            _ => unreachable!(),
        };
        ClusterRegistry::subscribe_kind(cx, CONTEXT, client, &apps("Deployment", "deployments"));
    });
    for _ in 0..400 {
        cx.run_until_parked();
        if cx.update(|cx| applies(&deployment, &web, CONTEXT, cx)) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("never offered once the Deployment was cached");
}

/// On a workload: an any-pod entry, an entry for that workload, and a selector
/// entry one of its cached pods matches - not one for another workload.
#[gpui_kit::test]
async fn a_workload_row_is_offered_its_own_and_its_pods_entries(cx: &mut TestAppContext) {
    let _cluster = cluster(cx, &[apps("Deployment", "deployments")]);
    let web = Resource::Workload {
        kind: WorkloadKind::Deployment,
        namespace: "shop".into(),
        name: "web".into(),
        selector: None,
    };
    let entries = [
        entry("Anywhere", Target::Any),
        entry("This deployment", web_deployment()),
        entry(
            "Another deployment",
            Target::Workload {
                namespace: "shop".into(),
                workload_kind: WorkloadKind::Deployment,
                name: "api".into(),
            },
        ),
        entry("Its pods", selector(Some("shop"), "tier=front")),
        entry("Not its pods", selector(None, "app=db")),
    ];
    let offered: Vec<String> = cx.update(|cx| {
        entries
            .iter()
            .filter(|entry| applies(entry, &web, CONTEXT, cx))
            .map(|entry| entry.name.clone())
            .collect()
    });
    assert_eq!(offered, ["Anywhere", "This deployment", "Its pods"]);
}

/// Spec "Pod prefill drops hash labels".
#[gpui_kit::test]
async fn a_pods_prefill_drops_the_hash_labels(cx: &mut TestAppContext) {
    let _cluster = cluster(cx, &[]);
    let web = Resource::Pod(Box::new(pod(
        json!({ "app": "web", "pod-template-hash": "7d9", "controller-revision-hash": "x" }),
        Some(("ReplicaSet", "web-7d9")),
    )));
    let target = cx.update(|cx| prefill(&web, CONTEXT, cx));
    assert_eq!(target, selector(Some("shop"), "app=web"));
}

/// Spec "Owner selector": with the ReplicaSet and Deployment cached, the
/// Deployment's selector - not the pod's own labels.
#[gpui_kit::test]
async fn a_pods_prefill_takes_its_deployments_selector_when_cached(cx: &mut TestAppContext) {
    let _cluster = cluster(
        cx,
        &[
            apps("ReplicaSet", "replicasets"),
            apps("Deployment", "deployments"),
        ],
    );
    let web = Resource::Pod(Box::new(pod(
        json!({ "app": "web", "tier": "front", "pod-template-hash": "7d9", "extra": "x" }),
        Some(("ReplicaSet", "web-7d9")),
    )));
    let target = cx.update(|cx| prefill(&web, CONTEXT, cx));
    assert_eq!(target, selector(Some("shop"), "app=web,tier=front"));
}

#[test]
fn a_workloads_prefill_names_the_workload() {
    let web = Resource::Workload {
        kind: WorkloadKind::Deployment,
        namespace: "shop".into(),
        name: "web".into(),
        selector: None,
    };
    let app = gpui_kit::TestAppContext::single();
    let target = app.update(|cx| prefill(&web, CONTEXT, cx));
    assert_eq!(target, web_deployment());
}
