//! Workload controllers' sections - ReplicaSet, Deployment, StatefulSet,
//! DaemonSet and Job: how many pods they want and have, what they select, and
//! how they're doing. Owners (a ReplicaSet's Deployment) are Overview's.

use super::super::model::{FieldValue, ObjectField, ObjectSection};
use super::common::{condition_badges, non_empty, selector_chips};
use crate::k8s::object_ref::ObjectRef;
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet, StatefulSet};
use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;

/// The conditions on a workload that are bad news when they hold.
fn negative(condition: &str) -> bool {
    matches!(condition, "ReplicaFailure" | "Failed" | "FailureTarget")
}

fn selector(fields: &mut Vec<ObjectField>, selector: Option<&LabelSelector>) {
    let chips = selector.map(selector_chips).unwrap_or_default();
    if !chips.is_empty() {
        fields.push(ObjectField::new("Selector", FieldValue::Chips(chips)));
    }
}

/// `"desired 3 · ready 2 · available 2"`, skipping counts the API left out.
fn counts(parts: &[(&str, Option<i32>)]) -> String {
    parts
        .iter()
        .filter_map(|(name, count)| count.map(|count| format!("{name} {count}")))
        .collect::<Vec<_>>()
        .join(" · ")
}

pub(super) fn replica_set(set: &ReplicaSet) -> Vec<ObjectSection> {
    let status = set.status.as_ref();
    let mut fields = vec![ObjectField::text(
        "Replicas",
        counts(&[
            ("desired", set.spec.as_ref().and_then(|spec| spec.replicas)),
            ("current", status.map(|status| status.replicas)),
            ("ready", status.and_then(|status| status.ready_replicas)),
            (
                "available",
                status.and_then(|status| status.available_replicas),
            ),
        ]),
    )];
    selector(&mut fields, set.spec.as_ref().map(|spec| &spec.selector));
    let conditions = status
        .into_iter()
        .flat_map(|status| status.conditions.iter().flatten())
        .map(|condition| (condition.type_.as_str(), condition.status.as_str()));
    if let Some(badges) = condition_badges(conditions, negative) {
        fields.push(ObjectField::new("Conditions", badges));
    }
    vec![ObjectSection::new("Replicas", fields)]
}

pub(super) fn deployment(deployment: &Deployment) -> Vec<ObjectSection> {
    let spec = deployment.spec.as_ref();
    let status = deployment.status.as_ref();
    let mut fields = vec![ObjectField::text(
        "Replicas",
        counts(&[
            ("desired", spec.and_then(|spec| spec.replicas)),
            ("updated", status.and_then(|status| status.updated_replicas)),
            ("ready", status.and_then(|status| status.ready_replicas)),
            (
                "available",
                status.and_then(|status| status.available_replicas),
            ),
        ]),
    )];
    if let Some(strategy) = non_empty(
        spec.and_then(|spec| spec.strategy.as_ref())
            .and_then(|strategy| strategy.type_.as_deref()),
    ) {
        fields.push(ObjectField::text("Strategy", strategy));
    }
    selector(&mut fields, spec.map(|spec| &spec.selector));
    let conditions = status
        .into_iter()
        .flat_map(|status| status.conditions.iter().flatten())
        .map(|condition| (condition.type_.as_str(), condition.status.as_str()));
    if let Some(badges) = condition_badges(conditions, negative) {
        fields.push(ObjectField::new("Conditions", badges));
    }
    vec![ObjectSection::new("Replicas", fields)]
}

/// A StatefulSet also names the headless Service that gives its pods their
/// network identity - a reference.
pub(super) fn stateful_set(set: &StatefulSet, namespace: &str) -> Vec<ObjectSection> {
    let spec = set.spec.as_ref();
    let status = set.status.as_ref();
    let mut fields = vec![ObjectField::text(
        "Replicas",
        counts(&[
            ("desired", spec.and_then(|spec| spec.replicas)),
            ("current", status.and_then(|status| status.current_replicas)),
            ("ready", status.and_then(|status| status.ready_replicas)),
            (
                "available",
                status.and_then(|status| status.available_replicas),
            ),
        ]),
    )];
    if let Some(service) = non_empty(spec.and_then(|spec| spec.service_name.as_deref())) {
        fields.push(ObjectField::references(
            "Service",
            vec![ObjectRef::core("Service", namespace, service)],
            false,
        ));
    }
    selector(&mut fields, spec.map(|spec| &spec.selector));
    vec![ObjectSection::new("Replicas", fields)]
}

pub(super) fn daemon_set(set: &DaemonSet) -> Vec<ObjectSection> {
    let status = set.status.as_ref();
    let mut fields = vec![ObjectField::text(
        "Scheduled",
        counts(&[
            (
                "desired",
                status.map(|status| status.desired_number_scheduled),
            ),
            (
                "current",
                status.map(|status| status.current_number_scheduled),
            ),
            ("ready", status.map(|status| status.number_ready)),
            (
                "available",
                status.and_then(|status| status.number_available),
            ),
        ]),
    )];
    if let Some(misscheduled) = status
        .map(|status| status.number_misscheduled)
        .filter(|count| *count > 0)
    {
        fields.push(ObjectField::text("Misscheduled", misscheduled.to_string()));
    }
    selector(&mut fields, set.spec.as_ref().map(|spec| &spec.selector));
    vec![ObjectSection::new("Pods", fields)]
}

pub(super) fn job(job: &Job) -> Vec<ObjectSection> {
    let spec = job.spec.as_ref();
    let status = job.status.as_ref();
    let mut fields = vec![ObjectField::text(
        "Completions",
        counts(&[
            ("wanted", spec.and_then(|spec| spec.completions)),
            ("succeeded", status.and_then(|status| status.succeeded)),
            ("active", status.and_then(|status| status.active)),
            ("failed", status.and_then(|status| status.failed)),
        ]),
    )];
    if let Some(parallelism) = spec.and_then(|spec| spec.parallelism) {
        fields.push(ObjectField::text("Parallelism", parallelism.to_string()));
    }
    if let Some(started) = status.and_then(|status| status.start_time.as_ref()) {
        fields.push(ObjectField::text("Started", started.0.to_string()));
    }
    if let Some(completed) = status.and_then(|status| status.completion_time.as_ref()) {
        fields.push(ObjectField::text("Completed", completed.0.to_string()));
    }
    let conditions = status
        .into_iter()
        .flat_map(|status| status.conditions.iter().flatten())
        .map(|condition| (condition.type_.as_str(), condition.status.as_str()));
    if let Some(badges) = condition_badges(conditions, negative) {
        fields.push(ObjectField::new("Conditions", badges));
    }
    selector(&mut fields, spec.and_then(|spec| spec.selector.as_ref()));
    vec![ObjectSection::new("Job", fields)]
}
