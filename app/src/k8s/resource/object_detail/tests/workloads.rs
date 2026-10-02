//! Workload controllers' sections (`resource-links` 6.3).

use super::fixtures::{deployments, kind, object, owned_replica_set, replica_sets};
use super::sections::field;
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::object_detail::model::FieldValue;
use crate::k8s::resource::object_detail::sections::sections_for;
use crate::ui::detail::BadgeTone;
use serde_json::json;

#[test]
fn a_replica_set_shows_its_replicas_and_selector() {
    let sections = sections_for(&replica_sets(), &owned_replica_set());

    assert_eq!(
        field(&sections, "Replicas").value.text(),
        "desired 2 · current 2 · ready 1 · available 1"
    );
    assert_eq!(field(&sections, "Selector").value.text(), "app=web");
}

#[test]
fn a_deployment_shows_replicas_strategy_selector_and_conditions() {
    let deployment = object(json!({
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": { "name": "web", "namespace": "staging" },
        "spec": {
            "replicas": 3,
            "strategy": { "type": "RollingUpdate" },
            "selector": {
                "matchLabels": { "app": "web" },
                "matchExpressions": [{ "key": "tier", "operator": "In", "values": ["front"] }],
            },
            "template": {},
        },
        "status": {
            "updatedReplicas": 3, "readyReplicas": 2, "availableReplicas": 2,
            "conditions": [
                { "type": "Available", "status": "True" },
                { "type": "ReplicaFailure", "status": "True" },
            ],
        },
    }));

    let sections = sections_for(&deployments(), &deployment);

    assert_eq!(
        field(&sections, "Replicas").value.text(),
        "desired 3 · updated 3 · ready 2 · available 2"
    );
    assert_eq!(field(&sections, "Strategy").value.text(), "RollingUpdate");
    assert_eq!(
        field(&sections, "Selector").value.text(),
        "app=web, tier In (front)"
    );
    assert_eq!(
        field(&sections, "Conditions").value,
        FieldValue::Badges(vec![
            ("Available".into(), BadgeTone::Good),
            ("ReplicaFailure".into(), BadgeTone::Warning),
        ])
    );
}

#[test]
fn a_stateful_set_references_its_service() {
    let set = object(json!({
        "apiVersion": "apps/v1",
        "kind": "StatefulSet",
        "metadata": { "name": "db", "namespace": "staging" },
        "spec": {
            "replicas": 3,
            "serviceName": "db-headless",
            "selector": { "matchLabels": { "app": "db" } },
            "template": {},
        },
        "status": { "replicas": 3, "readyReplicas": 3, "currentReplicas": 3 },
    }));

    let sections = sections_for(&kind("apps", "v1", "StatefulSet", true), &set);

    assert_eq!(
        field(&sections, "Service").value,
        FieldValue::References {
            targets: vec![ObjectRef::core("Service", "staging", "db-headless")],
            qualified: false,
        }
    );
    assert_eq!(
        field(&sections, "Replicas").value.text(),
        "desired 3 · current 3 · ready 3"
    );
}

#[test]
fn a_daemon_set_shows_its_scheduling() {
    let set = object(json!({
        "apiVersion": "apps/v1",
        "kind": "DaemonSet",
        "metadata": { "name": "agent", "namespace": "kube-system" },
        "spec": { "selector": { "matchLabels": { "app": "agent" } }, "template": {} },
        "status": {
            "desiredNumberScheduled": 4, "currentNumberScheduled": 4,
            "numberReady": 3, "numberAvailable": 3, "numberMisscheduled": 1,
        },
    }));

    let sections = sections_for(&kind("apps", "v1", "DaemonSet", true), &set);

    assert_eq!(
        field(&sections, "Scheduled").value.text(),
        "desired 4 · current 4 · ready 3 · available 3"
    );
    assert_eq!(field(&sections, "Misscheduled").value.text(), "1");
}

#[test]
fn a_failed_job_shows_its_counts_and_a_warning() {
    let job = object(json!({
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": { "name": "migrate", "namespace": "staging" },
        "spec": { "completions": 1, "parallelism": 1, "template": {} },
        "status": {
            "failed": 2,
            "conditions": [{ "type": "Failed", "status": "True" }],
        },
    }));

    let sections = sections_for(&kind("batch", "v1", "Job", true), &job);

    assert_eq!(
        field(&sections, "Completions").value.text(),
        "wanted 1 · failed 2"
    );
    assert_eq!(
        field(&sections, "Conditions").value,
        FieldValue::Badges(vec![("Failed".into(), BadgeTone::Warning)])
    );
}
