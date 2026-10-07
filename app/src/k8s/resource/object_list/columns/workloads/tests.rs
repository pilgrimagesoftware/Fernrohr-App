//! Workload kinds' cells from fixture objects (`standard-resource-panels` 2.2).

use super::{CRON_JOB, DAEMON_SET, DEPLOYMENT, JOB, REPLICA_SET, STATEFUL_SET};
use crate::k8s::resource::object_list::columns::{Cell, KindColumns};
use crate::ui::style::Tone;
use jiff::Timestamp;
use kube::api::DynamicObject;
use serde_json::json;

/// `columns`' cells for the object `json` describes.
fn cells(columns: &KindColumns, json: serde_json::Value) -> Vec<Cell> {
    let object: DynamicObject = serde_json::from_value(json).expect("a valid object");
    columns.cells_for(&object)
}

fn at(time: &str) -> Timestamp {
    time.parse().unwrap()
}

#[test]
fn a_deployment_shows_ready_up_to_date_and_available() {
    let cells = cells(
        &DEPLOYMENT,
        json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": { "name": "web", "namespace": "staging" },
            "spec": { "replicas": 3, "selector": {}, "template": {} },
            "status": { "readyReplicas": 2, "updatedReplicas": 3, "availableReplicas": 2 },
        }),
    );
    assert_eq!(
        cells,
        vec![Cell::Readiness(2, 3), Cell::Number(3), Cell::Number(2)]
    );
}

/// A Deployment scaled to zero has no ready replicas to report: the API
/// leaves the counts out, and they read as 0.
#[test]
fn a_scaled_down_deployment_reads_as_zero() {
    let cells = cells(
        &DEPLOYMENT,
        json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": { "name": "web", "namespace": "staging" },
            "spec": { "replicas": 0, "selector": {}, "template": {} },
            "status": {},
        }),
    );
    assert_eq!(
        cells,
        vec![Cell::Readiness(0, 0), Cell::Number(0), Cell::Number(0)]
    );
}

#[test]
fn a_replica_set_shows_desired_current_and_ready() {
    let cells = cells(
        &REPLICA_SET,
        json!({
            "apiVersion": "apps/v1", "kind": "ReplicaSet",
            "metadata": { "name": "web-7d9f", "namespace": "staging" },
            "spec": { "replicas": 2, "selector": {} },
            "status": { "replicas": 2, "readyReplicas": 1 },
        }),
    );
    assert_eq!(
        cells,
        vec![Cell::Number(2), Cell::Number(2), Cell::Number(1)]
    );
}

#[test]
fn a_stateful_set_shows_ready_of_desired() {
    let cells = cells(
        &STATEFUL_SET,
        json!({
            "apiVersion": "apps/v1", "kind": "StatefulSet",
            "metadata": { "name": "db", "namespace": "staging" },
            "spec": { "replicas": 3, "serviceName": "db", "selector": {}, "template": {} },
            "status": { "replicas": 3, "readyReplicas": 3 },
        }),
    );
    assert_eq!(cells, vec![Cell::Readiness(3, 3)]);
    assert_eq!(cells[0].tone(), Some(Tone::Good));
}

#[test]
fn a_daemon_set_shows_its_five_counts() {
    let cells = cells(
        &DAEMON_SET,
        json!({
            "apiVersion": "apps/v1", "kind": "DaemonSet",
            "metadata": { "name": "agent", "namespace": "kube-system" },
            "spec": { "selector": {}, "template": {} },
            "status": {
                "desiredNumberScheduled": 4, "currentNumberScheduled": 4, "numberReady": 3,
                "updatedNumberScheduled": 4, "numberAvailable": 3, "numberMisscheduled": 0,
            },
        }),
    );
    assert_eq!(
        cells,
        vec![
            Cell::Number(4),
            Cell::Number(4),
            Cell::Number(3),
            Cell::Number(4),
            Cell::Number(3),
        ]
    );
}

/// A finished Job shows `Complete`, its completions, and how long it ran - a
/// fixed span, not a growing one.
#[test]
fn a_finished_job_shows_its_status_completions_and_run_time() {
    let cells = cells(
        &JOB,
        json!({
            "apiVersion": "batch/v1", "kind": "Job",
            "metadata": { "name": "migrate", "namespace": "staging" },
            "spec": { "completions": 1, "template": {} },
            "status": {
                "succeeded": 1,
                "startTime": "2026-10-02T11:00:00Z",
                "completionTime": "2026-10-02T11:01:30Z",
                "conditions": [{ "type": "Complete", "status": "True" }],
            },
        }),
    );
    assert_eq!(
        cells,
        vec![
            Cell::status("Complete", Tone::Good),
            Cell::Ratio(1, 1),
            Cell::Duration(90),
        ]
    );
}

/// A running Job's duration grows: it is measured from its start, like Age.
#[test]
fn a_running_job_measures_its_duration_live() {
    let cells = cells(
        &JOB,
        json!({
            "apiVersion": "batch/v1", "kind": "Job",
            "metadata": { "name": "backfill", "namespace": "staging" },
            "spec": { "completions": 5, "template": {} },
            "status": { "active": 1, "succeeded": 2, "startTime": "2026-10-02T11:00:00Z" },
        }),
    );
    assert_eq!(
        cells,
        vec![
            Cell::status("Running", Tone::Info),
            Cell::Ratio(2, 5),
            Cell::Age(at("2026-10-02T11:00:00Z")),
        ]
    );
}

#[test]
fn a_failed_or_suspended_job_says_so() {
    let failed = cells(
        &JOB,
        json!({
            "apiVersion": "batch/v1", "kind": "Job",
            "metadata": { "name": "bad", "namespace": "staging" },
            "spec": { "template": {} },
            "status": { "failed": 1, "conditions": [{ "type": "Failed", "status": "True" }] },
        }),
    );
    let suspended = cells(
        &JOB,
        json!({
            "apiVersion": "batch/v1", "kind": "Job",
            "metadata": { "name": "held", "namespace": "staging" },
            "spec": { "suspend": true, "template": {} },
        }),
    );
    assert_eq!(failed[0], Cell::status("Failed", Tone::Bad));
    assert_eq!(suspended[0], Cell::status("Suspended", Tone::Neutral));
    assert_eq!(
        suspended[2],
        Cell::Empty,
        "a Job that never started ran for no time"
    );
}

#[test]
fn a_cron_job_shows_schedule_suspend_active_and_last_schedule() {
    let cells = cells(
        &CRON_JOB,
        json!({
            "apiVersion": "batch/v1", "kind": "CronJob",
            "metadata": { "name": "backup", "namespace": "staging" },
            "spec": { "schedule": "0 3 * * *", "jobTemplate": { "spec": { "template": {} } } },
            "status": {
                "active": [{ "kind": "Job", "name": "backup-1" }],
                "lastScheduleTime": "2026-10-02T03:00:00Z",
            },
        }),
    );
    assert_eq!(
        cells,
        vec![
            Cell::text("0 3 * * *"),
            Cell::text("False"),
            Cell::Number(1),
            Cell::Age(at("2026-10-02T03:00:00Z")),
        ]
    );
}

/// An object that doesn't deserialize as its kind gets every column empty,
/// never a panic.
#[test]
fn a_malformed_deployment_gets_empty_cells() {
    let cells = cells(
        &DEPLOYMENT,
        json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": { "name": "web", "namespace": "staging" },
            "spec": { "replicas": "three" },
        }),
    );
    assert_eq!(cells, vec![Cell::Empty, Cell::Empty, Cell::Empty]);
}
