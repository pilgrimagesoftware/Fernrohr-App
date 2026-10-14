//! A drain: cordon, the node's pods, and which of them are evicted (task 5.3).

use super::*;
use crate::k8s::resource::bulk::Mutation;
use crate::k8s::test_recorder::Recorded;
use serde_json::{Value, json};

/// A drain whose evictions give up quickly: a refused eviction is retried by
/// the client until then.
fn drain() -> Mutation {
    Mutation::Drain {
        eviction_timeout: std::time::Duration::from_millis(300),
    }
}

/// A pod on `node-a`, owned by `owner` (a controller of that kind), or by
/// nothing.
fn pod(name: &str, owner: Option<&str>, mirror: bool) -> Value {
    let mut pod = json!({ "apiVersion": "v1", "kind": "Pod",
        "metadata": { "name": name, "namespace": "shop" },
        "spec": { "nodeName": "node-a", "containers": [] } });
    if let Some(owner) = owner {
        pod["metadata"]["ownerReferences"] = json!([{ "apiVersion": "apps/v1", "kind": owner,
            "name": "owner", "uid": "uid-owner", "controller": true }]);
    }
    if mirror {
        pod["metadata"]["annotations"] = json!({ "kubernetes.io/config.mirror": "hash" });
    }
    pod
}

fn drain_cluster(
    pods: Vec<Value>,
    refuse: Option<&'static str>,
) -> impl Fn(&Recorded) -> (&'static str, Value) + Send + Sync + 'static {
    move |request| {
        let target = path(&request.target);
        if request.method == "PATCH" {
            return ("200 OK", object(&node(), "node-a"));
        }
        if target == "/api/v1/pods" {
            return (
                "200 OK",
                json!({ "apiVersion": "v1", "kind": "PodList", "metadata": {}, "items": pods }),
            );
        }
        if refuse.is_some_and(|name| target.ends_with(&format!("/pods/{name}/eviction"))) {
            return (
                "429 Too Many Requests",
                refusal(
                    429,
                    "TooManyRequests",
                    "Cannot evict pod as it would violate the pod's disruption budget.",
                ),
            );
        }
        ("201 Created", success())
    }
}

#[test]
fn drain_cordons_then_evicts_all_but_daemonset_and_mirror_pods() {
    let pods = vec![
        pod("web-1", Some("ReplicaSet"), false),
        pod("agent-x", Some("DaemonSet"), false),
        pod("kube-proxy-node-a", None, true),
    ];
    let (requests, outcomes) = record(
        drain(),
        &[target(node(), "node-a")],
        drain_cluster(pods, None),
    );

    assert_eq!(requests[0].method, "PATCH", "cordoned first");
    assert_eq!(
        requests[0].json(),
        json!({ "spec": { "unschedulable": true } })
    );
    assert!(
        requests[1]
            .target
            .contains("fieldSelector=spec.nodeName%3Dnode-a"),
        "the node's pods: {}",
        requests[1].target
    );
    let evictions: Vec<&str> = requests
        .iter()
        .filter(|request| request.method == "POST")
        .map(|request| path(&request.target))
        .collect();
    assert_eq!(evictions, ["/api/v1/namespaces/shop/pods/web-1/eviction"]);
    assert_eq!(outcomes.len(), 1);
    assert!(outcomes[0].result.is_ok());
}

#[test]
fn a_disruption_budget_fails_one_eviction_without_stopping_the_drain() {
    let pods = vec![
        pod("web-1", Some("ReplicaSet"), false),
        pod("web-2", Some("ReplicaSet"), false),
        pod("web-3", Some("ReplicaSet"), false),
    ];
    let (requests, outcomes) = record(
        drain(),
        &[target(node(), "node-a")],
        drain_cluster(pods, Some("web-2")),
    );

    let evictions = requests
        .iter()
        .filter(|request| request.method == "POST")
        .count();
    assert!(evictions >= 3, "web-3 is evicted after web-2 is refused");
    assert!(
        requests
            .iter()
            .any(|request| path(&request.target).ends_with("/pods/web-3/eviction")),
        "web-3 is evicted too"
    );
    assert!(
        !requests
            .iter()
            .any(|request| request.json() == json!({ "spec": { "unschedulable": null } })),
        "the node stays cordoned"
    );
    assert!(outcomes[0].result.is_ok(), "the node itself was cordoned");
    assert_eq!(outcomes.len(), 2);
    assert_eq!(outcomes[1].part.as_deref(), Some("evict shop/web-2"));
    let failure = outcomes[1].result.as_ref().unwrap_err();
    assert!(
        failure.detail.to_lowercase().contains("disruption"),
        "says why: {failure:?}"
    );
}

#[test]
fn drain_reports_a_pod_no_controller_manages_instead_of_evicting_it() {
    let pods = vec![pod("scratch", None, false)];
    let (requests, outcomes) = record(
        drain(),
        &[target(node(), "node-a")],
        drain_cluster(pods, None),
    );
    assert!(!requests.iter().any(|request| request.method == "POST"));
    assert_eq!(outcomes.len(), 2);
    assert_eq!(outcomes[1].part.as_deref(), Some("evict shop/scratch"));
    assert!(outcomes[1].result.is_err());
}
