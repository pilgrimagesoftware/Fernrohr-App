//! Each mutation's requests, per object, and a batch with one failure (tasks
//! 4-6).

use super::*;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::bulk::{BulkTarget, MetadataMap, Mutation};
use crate::k8s::test_recorder::Recorded;
use serde_json::json;

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
