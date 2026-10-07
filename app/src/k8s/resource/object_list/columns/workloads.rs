//! Workload kinds' list columns (`standard-resource-panels` 2.2), as
//! `kubectl get` shows them: Deployment, ReplicaSet, StatefulSet, DaemonSet,
//! Job and CronJob.

use super::{Cell, ColumnDef, KindColumns, typed_cells};
use crate::k8s::resource::status_tone::JobStatus;
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet, StatefulSet};
use k8s_openapi::api::batch::v1::{CronJob, Job};

const fn column(id: &'static str, title: &'static str, width: f32) -> ColumnDef {
    ColumnDef { id, title, width }
}

const READY: ColumnDef = column("ready", "Ready", 70.);
const UP_TO_DATE: ColumnDef = column("up_to_date", "Up-to-date", 90.);
const AVAILABLE: ColumnDef = column("available", "Available", 80.);
const DESIRED: ColumnDef = column("desired", "Desired", 70.);
const CURRENT: ColumnDef = column("current", "Current", 70.);
const READY_COUNT: ColumnDef = column("ready", "Ready", 60.);

/// A replica count the API may leave out, as a number - unset reads as 0, as
/// `kubectl` shows it.
fn count(value: Option<i32>) -> Cell {
    Cell::Number(i64::from(value.unwrap_or(0)))
}

/// `ready/desired`, with an unset desired count reading as the API's default
/// of 1.
fn ready_of(ready: Option<i32>, desired: Option<i32>) -> Cell {
    Cell::Ratio(
        i64::from(ready.unwrap_or(0)),
        i64::from(desired.unwrap_or(1)),
    )
}

/// Ready replicas of desired, in their readiness tone - unset desired reading
/// as 1, as in [`ready_of`].
fn readiness_of(ready: Option<i32>, desired: Option<i32>) -> Cell {
    Cell::Readiness(
        i64::from(ready.unwrap_or(0)),
        i64::from(desired.unwrap_or(1)),
    )
}

pub(super) static DEPLOYMENT: KindColumns = KindColumns {
    columns: &[READY, UP_TO_DATE, AVAILABLE],
    cells: |object| {
        typed_cells::<Deployment>(object, |deployment| {
            let desired = deployment.spec.as_ref().and_then(|spec| spec.replicas);
            let status = deployment.status.as_ref();
            vec![
                readiness_of(status.and_then(|status| status.ready_replicas), desired),
                count(status.and_then(|status| status.updated_replicas)),
                count(status.and_then(|status| status.available_replicas)),
            ]
        })
    },
};

pub(super) static REPLICA_SET: KindColumns = KindColumns {
    columns: &[DESIRED, CURRENT, READY_COUNT],
    cells: |object| {
        typed_cells::<ReplicaSet>(object, |set| {
            let status = set.status.as_ref();
            vec![
                Cell::Number(i64::from(
                    set.spec
                        .as_ref()
                        .and_then(|spec| spec.replicas)
                        .unwrap_or(1),
                )),
                count(status.map(|status| status.replicas)),
                count(status.and_then(|status| status.ready_replicas)),
            ]
        })
    },
};

pub(super) static STATEFUL_SET: KindColumns = KindColumns {
    columns: &[READY],
    cells: |object| {
        typed_cells::<StatefulSet>(object, |set| {
            vec![readiness_of(
                set.status.as_ref().and_then(|status| status.ready_replicas),
                set.spec.as_ref().and_then(|spec| spec.replicas),
            )]
        })
    },
};

pub(super) static DAEMON_SET: KindColumns = KindColumns {
    columns: &[DESIRED, CURRENT, READY_COUNT, UP_TO_DATE, AVAILABLE],
    cells: |object| {
        typed_cells::<DaemonSet>(object, |set| {
            let status = set.status.as_ref();
            vec![
                count(status.map(|status| status.desired_number_scheduled)),
                count(status.map(|status| status.current_number_scheduled)),
                count(status.map(|status| status.number_ready)),
                count(status.and_then(|status| status.updated_number_scheduled)),
                count(status.and_then(|status| status.number_available)),
            ]
        })
    },
};

pub(super) static JOB: KindColumns = KindColumns {
    columns: &[
        column("status", "Status", 90.),
        column("completions", "Completions", 100.),
        column("duration", "Duration", 80.),
    ],
    cells: |object| {
        typed_cells::<Job>(object, |job| {
            let job_status = JobStatus::of(job);
            let status = job.status.as_ref();
            let started = status.and_then(|status| status.start_time.as_ref());
            let completed = status.and_then(|status| status.completion_time.as_ref());
            // A finished Job's run time is fixed; a running one's grows, so it
            // is measured live like Age. One that hasn't started has none.
            let duration = match (started, completed) {
                (Some(started), Some(completed)) => {
                    Cell::Duration(completed.0.duration_since(started.0).as_secs_f64() as i64)
                }
                (Some(started), None) => Cell::Age(started.0),
                (None, _) => Cell::Empty,
            };
            vec![
                Cell::status(job_status.to_string(), job_status.tone()),
                ready_of(
                    status.and_then(|status| status.succeeded),
                    job.spec.as_ref().and_then(|spec| spec.completions),
                ),
                duration,
            ]
        })
    },
};

pub(super) static CRON_JOB: KindColumns = KindColumns {
    columns: &[
        column("schedule", "Schedule", 120.),
        column("suspend", "Suspend", 70.),
        column("active", "Active", 60.),
        column("last_schedule", "Last schedule", 100.),
    ],
    cells: |object| {
        typed_cells::<CronJob>(object, |cron_job| {
            let status = cron_job.status.as_ref();
            let active = status
                .and_then(|status| status.active.as_ref())
                .map_or(0, Vec::len);
            vec![
                Cell::text(cron_job.spec.schedule.clone()),
                Cell::text(if cron_job.spec.suspend.unwrap_or(false) {
                    "True"
                } else {
                    "False"
                }),
                Cell::Number(active as i64),
                status
                    .and_then(|status| status.last_schedule_time.as_ref())
                    .map_or(Cell::Empty, |last| Cell::Age(last.0)),
            ]
        })
    },
};

#[cfg(test)]
mod tests;
