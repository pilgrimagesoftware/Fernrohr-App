//! Workload controllers' sections - ReplicaSet, Deployment, StatefulSet,
//! DaemonSet, Job and CronJob: how many pods they want and have, what they
//! select, and how they're doing. Owners (a ReplicaSet's Deployment) are
//! Overview's; a CronJob's active Jobs are references.

use super::super::model::{FieldValue, ObjectField, ObjectSection};
use super::common::{condition_badges, non_empty, selector_chips};
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::status_tone::{self, JobStatus};
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet, StatefulSet};
use k8s_openapi::api::batch::v1::{CronJob, Job};
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

/// The replica counts in their readiness tone: `ready` (unset reading as 0)
/// of `desired`.
fn replicas(
    label: &str,
    parts: &[(&str, Option<i32>)],
    ready: Option<i32>,
    desired: i32,
) -> ObjectField {
    ObjectField::status(
        label,
        counts(parts),
        status_tone::readiness(i64::from(ready.unwrap_or(0)), i64::from(desired)),
    )
}

/// An unset `replicas` is the API's default of 1.
const DEFAULT_REPLICAS: i32 = 1;

pub(super) fn replica_set(set: &ReplicaSet) -> Vec<ObjectSection> {
    let status = set.status.as_ref();
    let desired = set.spec.as_ref().and_then(|spec| spec.replicas);
    let ready = status.and_then(|status| status.ready_replicas);
    let mut fields = vec![replicas(
        "Replicas",
        &[
            ("desired", desired),
            ("current", status.map(|status| status.replicas)),
            ("ready", ready),
            (
                "available",
                status.and_then(|status| status.available_replicas),
            ),
        ],
        ready,
        desired.unwrap_or(DEFAULT_REPLICAS),
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
    let desired = spec.and_then(|spec| spec.replicas);
    let ready = status.and_then(|status| status.ready_replicas);
    let mut fields = vec![replicas(
        "Replicas",
        &[
            ("desired", desired),
            ("updated", status.and_then(|status| status.updated_replicas)),
            ("ready", ready),
            (
                "available",
                status.and_then(|status| status.available_replicas),
            ),
        ],
        ready,
        desired.unwrap_or(DEFAULT_REPLICAS),
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
    let desired = spec.and_then(|spec| spec.replicas);
    let ready = status.and_then(|status| status.ready_replicas);
    let mut fields = vec![replicas(
        "Replicas",
        &[
            ("desired", desired),
            ("current", status.and_then(|status| status.current_replicas)),
            ("ready", ready),
            (
                "available",
                status.and_then(|status| status.available_replicas),
            ),
        ],
        ready,
        desired.unwrap_or(DEFAULT_REPLICAS),
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
    let desired = status.map(|status| status.desired_number_scheduled);
    let ready = status.map(|status| status.number_ready);
    let mut fields = vec![replicas(
        "Scheduled",
        &[
            ("desired", desired),
            (
                "current",
                status.map(|status| status.current_number_scheduled),
            ),
            ("ready", ready),
            (
                "available",
                status.and_then(|status| status.number_available),
            ),
        ],
        ready,
        // A DaemonSet with no status yet has scheduled nothing to want.
        desired.unwrap_or(0),
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
    let job_status = JobStatus::of(job);
    let mut fields = vec![ObjectField::status(
        "Status",
        job_status.to_string(),
        job_status.tone(),
    )];
    fields.push(ObjectField::text(
        "Completions",
        counts(&[
            ("wanted", spec.and_then(|spec| spec.completions)),
            ("succeeded", status.and_then(|status| status.succeeded)),
            ("active", status.and_then(|status| status.active)),
            ("failed", status.and_then(|status| status.failed)),
        ]),
    ));
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

pub(super) fn cron_job(cron_job: &CronJob, namespace: &str) -> Vec<ObjectSection> {
    // Unlike most kinds', a CronJob's `spec` is required.
    let spec = &cron_job.spec;
    let status = cron_job.status.as_ref();
    let mut fields = Vec::new();

    if let Some(schedule) = non_empty(Some(spec.schedule.as_str())) {
        fields.push(ObjectField::text("Schedule", schedule));
    }
    if let Some(zone) = non_empty(spec.time_zone.as_deref()) {
        fields.push(ObjectField::text("Time Zone", zone));
    }
    // The API's default is "not suspended", so an unset flag reads as No.
    let suspended = spec.suspend.unwrap_or(false);
    fields.push(ObjectField::text(
        "Suspend",
        if suspended { "Yes" } else { "No" },
    ));
    if let Some(policy) = non_empty(spec.concurrency_policy.as_deref()) {
        fields.push(ObjectField::text("Concurrency Policy", policy));
    }
    if let Some(last) = status.and_then(|status| status.last_schedule_time.as_ref()) {
        fields.push(ObjectField::text("Last Schedule", last.0.to_string()));
    }
    if let Some(last) = status.and_then(|status| status.last_successful_time.as_ref()) {
        fields.push(ObjectField::text("Last Successful", last.0.to_string()));
    }
    let active: Vec<ObjectRef> = status
        .and_then(|status| status.active.as_ref())
        .into_iter()
        .flatten()
        .filter_map(|job| {
            let name = non_empty(job.name.as_deref())?;
            let namespace = non_empty(job.namespace.as_deref()).unwrap_or(namespace);
            Some(ObjectRef::namespaced("batch", "Job", namespace, name))
        })
        .collect();
    if !active.is_empty() {
        fields.push(ObjectField::references("Active Jobs", active, false));
    }

    vec![ObjectSection::new("Schedule", fields)]
}
