//! The bulk actions' rules and requests: which kinds each is offered for, how a
//! checked set is named, and - against a server that records what it is sent -
//! the request each mutation makes per object, and how a batch reports an object
//! that failed beside the ones that didn't.

use super::{BulkAction, BulkTarget, MetadataMap, Mutation, count_of_kinds, run};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::test_recorder::{Recorded, Recorder};
use kube::core::GroupVersionKind;
use serde_json::{Value, json};

fn kind(group: &str, version: &str, kind: &str, plural: &str, namespaced: bool) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk(group, version, kind),
        plural: plural.to_string(),
        namespaced,
        verbs: Default::default(),
    }
}

fn deployment() -> DiscoveredKind {
    kind("apps", "v1", "Deployment", "deployments", true)
}
fn daemon_set() -> DiscoveredKind {
    kind("apps", "v1", "DaemonSet", "daemonsets", true)
}
fn stateful_set() -> DiscoveredKind {
    kind("apps", "v1", "StatefulSet", "statefulsets", true)
}
fn replica_set() -> DiscoveredKind {
    kind("apps", "v1", "ReplicaSet", "replicasets", true)
}
fn node() -> DiscoveredKind {
    kind("", "v1", "Node", "nodes", false)
}
fn config_map() -> DiscoveredKind {
    kind("", "v1", "ConfigMap", "configmaps", true)
}
fn service() -> DiscoveredKind {
    kind("", "v1", "Service", "services", true)
}

fn target(kind: DiscoveredKind, name: &str) -> BulkTarget {
    let namespace = kind.namespaced.then(|| "shop".to_string());
    BulkTarget {
        kind,
        namespace,
        name: name.to_string(),
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap()
}

/// An object of `kind` named `name`, as the API answers a patch.
fn object(kind: &DiscoveredKind, name: &str) -> Value {
    let api_version = if kind.gvk.group.is_empty() {
        kind.gvk.version.clone()
    } else {
        format!("{}/{}", kind.gvk.group, kind.gvk.version)
    };
    json!({ "apiVersion": api_version, "kind": kind.gvk.kind, "metadata": { "name": name } })
}

fn success() -> Value {
    json!({ "kind": "Status", "apiVersion": "v1", "status": "Success", "code": 200 })
}

fn refusal(code: u16, reason: &str, message: &str) -> Value {
    json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure",
        "reason": reason, "code": code, "message": message })
}

fn offered(kinds: &[DiscoveredKind]) -> Vec<BulkAction> {
    BulkAction::offered(kinds)
}

// Eligibility (tasks 2.1).

#[test]
fn restart_and_rollback_need_every_object_to_be_a_rollout_kind() {
    let mixed = offered(&[deployment(), stateful_set(), daemon_set()]);
    assert!(mixed.contains(&BulkAction::RestartRollout), "{mixed:?}");
    assert!(mixed.contains(&BulkAction::RollbackRollout), "{mixed:?}");

    let with_service = offered(&[deployment(), service()]);
    assert!(!with_service.contains(&BulkAction::RestartRollout));
    assert!(!with_service.contains(&BulkAction::RollbackRollout));
    assert!(!offered(&[replica_set()]).contains(&BulkAction::RestartRollout));
}

#[test]
fn scale_needs_every_object_to_be_scalable() {
    assert!(offered(&[deployment(), replica_set(), stateful_set()]).contains(&BulkAction::Scale));
    assert!(!offered(&[deployment(), DiscoveredKind::pods()]).contains(&BulkAction::Scale));
    assert!(!offered(&[daemon_set()]).contains(&BulkAction::Scale));
}

#[test]
fn cordon_uncordon_and_drain_are_for_nodes_alone() {
    let nodes = offered(&[node(), node()]);
    for action in [BulkAction::Cordon, BulkAction::Uncordon, BulkAction::Drain] {
        assert!(nodes.contains(&action), "{action:?} for nodes");
        assert!(
            !offered(&[node(), DiscoveredKind::pods()]).contains(&action),
            "{action:?} with a pod checked too"
        );
    }
}

#[test]
fn view_logs_is_for_pods_alone() {
    assert!(offered(&[DiscoveredKind::pods()]).contains(&BulkAction::ViewLogs));
    assert!(!offered(&[deployment()]).contains(&BulkAction::ViewLogs));
}

#[test]
fn any_mix_still_offers_delete_label_annotate_and_the_copies() {
    let mixed = offered(&[DiscoveredKind::pods(), config_map(), service()]);
    assert_eq!(
        mixed,
        [
            BulkAction::Label,
            BulkAction::Annotate,
            BulkAction::CopyNames,
            BulkAction::CopyYaml,
            BulkAction::Delete,
        ]
    );
}

#[test]
fn a_kind_that_cant_be_patched_or_deleted_isnt_offered_them() {
    let mut read_only = deployment();
    read_only.verbs.patch = false;
    read_only.verbs.delete = false;
    assert_eq!(
        offered(&[read_only]),
        [BulkAction::CopyNames, BulkAction::CopyYaml]
    );
}

#[test]
fn nothing_is_offered_with_nothing_checked() {
    assert!(offered(&[]).is_empty());
}

#[test]
fn a_checked_set_is_named_by_how_many_of_each_kind() {
    let kinds = [deployment(), deployment(), daemon_set()];
    assert_eq!(count_of_kinds(&kinds), "2 Deployments, 1 DaemonSet");
    assert_eq!(
        count_of_kinds(&[DiscoveredKind::pods(), config_map()]),
        "1 Pod, 1 ConfigMap"
    );
    assert_eq!(count_of_kinds(&[node(), node()]), "2 Nodes");
}

// Requests (tasks 4-6).

/// Runs `mutation` on `targets` against a server answering with `answer`, and
/// returns the requests it got and the outcomes.
fn record(
    mutation: Mutation,
    targets: &[BulkTarget],
    answer: impl Fn(&Recorded) -> (&'static str, Value) + Send + Sync + 'static,
) -> (Vec<Recorded>, Vec<super::ObjectOutcome>) {
    let runtime = runtime();
    let (server, client) = Recorder::start_answering(runtime.handle(), answer);
    let outcomes = runtime.block_on(run(client, &mutation, targets));
    (server.requests(), outcomes)
}

fn sorted_targets(requests: &[Recorded]) -> Vec<String> {
    let mut targets: Vec<String> = requests
        .iter()
        .map(|request| format!("{} {}", request.method, path(&request.target)))
        .collect();
    targets.sort();
    targets
}

fn path(target: &str) -> &str {
    target.split('?').next().unwrap_or(target)
}

#[test]
fn bulk_delete_sends_the_single_row_delete_for_each_object() {
    let targets = [
        target(DiscoveredKind::pods(), "web-1"),
        target(config_map(), "settings"),
    ];
    let (requests, outcomes) = record(Mutation::Delete, &targets, |_| ("200 OK", success()));

    assert_eq!(
        sorted_targets(&requests),
        [
            "DELETE /api/v1/namespaces/shop/configmaps/settings",
            "DELETE /api/v1/namespaces/shop/pods/web-1",
        ]
    );
    for request in &requests {
        assert_eq!(
            request.json().get("gracePeriodSeconds"),
            None,
            "the normal delete, not a kill"
        );
    }
    assert!(outcomes.iter().all(|outcome| outcome.result.is_ok()));
}

#[test]
fn restart_stamps_each_rollout_kinds_template_with_the_same_time() {
    let at: jiff::Timestamp = "2026-10-14T12:00:00Z".parse().unwrap();
    let targets = [
        target(deployment(), "web"),
        target(daemon_set(), "agent"),
        target(stateful_set(), "db"),
    ];
    let kinds = [deployment(), daemon_set(), stateful_set()];
    let (requests, outcomes) = record(Mutation::RestartRollout { at }, &targets, move |request| {
        let kind = kinds
            .iter()
            .find(|kind| request.target.contains(&format!("/{}/", kind.plural)))
            .unwrap()
            .clone();
        ("200 OK", object(&kind, "x"))
    });

    assert_eq!(requests.len(), 3);
    for request in &requests {
        assert_eq!(request.method, "PATCH");
        let stamp = &request.json()["spec"]["template"]["metadata"]["annotations"]["kubectl.kubernetes.io/restartedAt"];
        let stamp: jiff::Timestamp = stamp.as_str().expect("an RFC 3339 time").parse().unwrap();
        assert_eq!(stamp, at, "{}", request.target);
    }
    assert!(outcomes.iter().all(|outcome| outcome.result.is_ok()));
}

#[test]
fn scale_sets_each_scalable_kinds_replicas_through_its_scale_subresource() {
    let targets = [
        target(deployment(), "web"),
        target(stateful_set(), "db"),
        target(replica_set(), "web-7d9"),
    ];
    let scale = json!({ "kind": "Scale", "apiVersion": "autoscaling/v1",
        "metadata": { "name": "x" }, "spec": { "replicas": 3 } });
    let (requests, outcomes) = record(Mutation::Scale { replicas: 3 }, &targets, move |_| {
        ("200 OK", scale.clone())
    });

    assert_eq!(
        sorted_targets(&requests),
        [
            "PATCH /apis/apps/v1/namespaces/shop/deployments/web/scale",
            "PATCH /apis/apps/v1/namespaces/shop/replicasets/web-7d9/scale",
            "PATCH /apis/apps/v1/namespaces/shop/statefulsets/db/scale",
        ]
    );
    for request in &requests {
        assert_eq!(request.json(), json!({ "spec": { "replicas": 3 } }));
    }
    assert!(outcomes.iter().all(|outcome| outcome.result.is_ok()));
}

#[test]
fn cordon_sets_unschedulable_and_uncordon_clears_it() {
    let targets = [target(node(), "node-a")];
    let (cordoned, _) = record(Mutation::Cordon, &targets, |_| {
        ("200 OK", object(&node(), "node-a"))
    });
    assert_eq!(cordoned[0].method, "PATCH");
    assert_eq!(path(&cordoned[0].target), "/api/v1/nodes/node-a");
    assert_eq!(
        cordoned[0].json(),
        json!({ "spec": { "unschedulable": true } })
    );

    let (uncordoned, _) = record(Mutation::Uncordon, &targets, |_| {
        ("200 OK", object(&node(), "node-a"))
    });
    assert_eq!(
        uncordoned[0].json(),
        json!({ "spec": { "unschedulable": null } })
    );
}

#[test]
fn label_and_annotate_set_one_key_and_an_empty_value_removes_it() {
    let targets = [target(DiscoveredKind::pods(), "web-1")];
    let answer = |_: &Recorded| ("200 OK", object(&DiscoveredKind::pods(), "web-1"));
    let set = |map, value: Option<&str>| Mutation::SetMetadata {
        map,
        key: "team".to_string(),
        value: value.map(str::to_string),
    };

    let (labelled, _) = record(set(MetadataMap::Labels, Some("platform")), &targets, answer);
    assert_eq!(
        labelled[0].json(),
        json!({ "metadata": { "labels": { "team": "platform" } } })
    );
    assert_eq!(
        labelled[0].content_type.as_deref(),
        Some("application/merge-patch+json")
    );

    let (annotated, _) = record(
        set(MetadataMap::Annotations, Some("platform")),
        &targets,
        answer,
    );
    assert_eq!(
        annotated[0].json(),
        json!({ "metadata": { "annotations": { "team": "platform" } } })
    );

    let (removed, _) = record(set(MetadataMap::Annotations, None), &targets, answer);
    assert_eq!(
        removed[0].json(),
        json!({ "metadata": { "annotations": { "team": null } } })
    );
}

#[test]
fn one_failure_in_a_batch_is_reported_beside_the_rest() {
    let targets: Vec<BulkTarget> = ["web-1", "web-2", "web-3"]
        .into_iter()
        .map(|name| target(DiscoveredKind::pods(), name))
        .collect();
    let (_, outcomes) = record(Mutation::Delete, &targets, |request| {
        if request.target.contains("/web-2") {
            (
                "403 Forbidden",
                refusal(403, "Forbidden", "pods \"web-2\" is forbidden"),
            )
        } else {
            ("200 OK", success())
        }
    });

    let names: Vec<(&str, bool)> = outcomes
        .iter()
        .map(|outcome| (outcome.target.name.as_str(), outcome.result.is_ok()))
        .collect();
    assert_eq!(
        names,
        [("web-1", true), ("web-2", false), ("web-3", true)],
        "every object's own outcome, in checked order"
    );
    let failure = outcomes[1].result.as_ref().unwrap_err();
    assert!(failure.detail.contains("forbidden"), "{failure:?}");
}

#[test]
fn a_rollback_with_no_earlier_revision_fails_that_object_alone() {
    // Each StatefulSet's ControllerRevisions: `db` has two, `cache` only its
    // current one.
    let revision = |owner: &str, revision: i64| {
        json!({ "apiVersion": "apps/v1", "kind": "ControllerRevision",
            "metadata": { "name": format!("{owner}-{revision}"), "namespace": "shop",
                "ownerReferences": [{ "apiVersion": "apps/v1", "kind": "StatefulSet",
                    "name": owner, "uid": format!("uid-{owner}"), "controller": true }] },
            "revision": revision,
            "data": { "spec": { "template": { "$patch": "replace",
                "metadata": { "labels": { "revision": revision.to_string() } } } } } })
    };
    let targets = [
        target(stateful_set(), "db"),
        target(stateful_set(), "cache"),
    ];
    let (requests, outcomes) = record(Mutation::RollbackRollout, &targets, move |request| {
        let target = path(&request.target);
        if target.ends_with("/controllerrevisions") {
            let items = vec![revision("db", 1), revision("db", 2), revision("cache", 1)];
            return (
                "200 OK",
                json!({ "apiVersion": "apps/v1", "kind": "ControllerRevisionList",
                    "metadata": {}, "items": items }),
            );
        }
        let name = target.rsplit('/').next().unwrap_or_default();
        (
            "200 OK",
            json!({ "apiVersion": "apps/v1", "kind": "StatefulSet",
                "metadata": { "name": name, "namespace": "shop", "uid": format!("uid-{name}"),
                    "resourceVersion": "7" } }),
        )
    });

    let results: Vec<(&str, bool)> = outcomes
        .iter()
        .map(|outcome| (outcome.target.name.as_str(), outcome.result.is_ok()))
        .collect();
    assert_eq!(results, [("db", true), ("cache", false)]);
    let patches: Vec<&Recorded> = requests
        .iter()
        .filter(|request| request.method == "PATCH")
        .collect();
    assert_eq!(patches.len(), 1, "only db is rolled back");
    assert_eq!(
        patches[0].json()["spec"]["template"]["metadata"]["labels"]["revision"],
        json!("1"),
        "back to db's previous revision's template"
    );
}

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
