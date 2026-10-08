//! A pod as a list row, and the view pipeline over rows: namespace scope, name filter and sort.

use super::*;
use crate::ui::style::Tone;
use k8s_openapi::api::core::v1::PodStatus;

/// The subset of a `Pod` a list row needs, computed fresh from the object at
/// render time rather than stored separately - see design D2 on field
/// pruning.
#[derive(Debug, Clone, PartialEq)]
pub struct PodRow {
    pub name: String,
    pub namespace: String,
    pub ready: String,
    pub status: String,
    pub restarts: i32,
    pub age: String,
    pub pod_ip: String,
    pub node: String,
    /// Raw seconds behind `age`'s display string - kept separately because
    /// the display string ("9m" vs "10m") doesn't sort correctly as text.
    pub age_secs: i64,
    /// How healthy the pod is, for the Status cell's colour ([`status_tone`]).
    pub status_tone: Tone,
    /// Whether its containers are ready, for the Ready cell's dot.
    pub ready_tone: Tone,
    /// How alarming its restarts are, for the Restarts cell's colour
    /// ([`restart_tone`]).
    pub restart_tone: Tone,
    /// How many port-forwards reach it (`port-forward-indicators` 2.1) - not the
    /// Pod's own, so `pod_row` leaves it 0 and the panel fills it in.
    pub forwards: usize,
}

/// Container waiting reasons that mean the pod won't run as it is - `kubectl`
/// shows these in its STATUS column in place of the phase.
pub(crate) const BAD_WAITING_REASONS: &[&str] = &[
    "CrashLoopBackOff",
    "ImagePullBackOff",
    "ErrImagePull",
    "Error",
    "CreateContainerConfigError",
    "CreateContainerError",
    "InvalidImageName",
    "RunContainerError",
];

/// A pod's health as a tone: `Failed`, or any container (init or main) waiting
/// for a reason in [`BAD_WAITING_REASONS`], is bad; `Pending` is a warning;
/// `Running` is good; `Succeeded` and anything unknown are neutral.
pub fn status_tone(status: Option<&PodStatus>) -> Tone {
    let Some(status) = status else {
        return Tone::Neutral;
    };
    let stuck = status
        .container_statuses
        .iter()
        .chain(status.init_container_statuses.iter())
        .flatten()
        .filter_map(|container| {
            container
                .state
                .as_ref()?
                .waiting
                .as_ref()?
                .reason
                .as_deref()
        })
        .any(|reason| BAD_WAITING_REASONS.contains(&reason));
    match status.phase.as_deref() {
        _ if stuck => Tone::Bad,
        Some("Failed") => Tone::Bad,
        Some("Pending") => Tone::Warning,
        Some("Running") => Tone::Good,
        _ => Tone::Neutral,
    }
}

/// All containers ready is good, some not is a warning, none at all neutral -
/// and a pod that ran to completion (`phase` `Succeeded`) is neutral: its
/// containers exited as they should, so none being ready is no warning.
pub(super) fn ready_tone(ready: usize, total: usize, phase: Option<&str>) -> Tone {
    match (ready, total) {
        _ if phase == Some("Succeeded") => Tone::Neutral,
        (_, 0) => Tone::Neutral,
        (ready, total) if ready == total => Tone::Good,
        _ => Tone::Warning,
    }
}

/// #121: a restart that finished within [`RECENT_RESTART_WINDOW`] is bad (red),
/// more than [`MANY_RESTARTS`] serious (orange), any at all a warning (yellow),
/// and none neutral. `since_last` is how long ago the last restart finished, if
/// any container reports one.
///
/// [`RECENT_RESTART_WINDOW`]: crate::consts::RECENT_RESTART_WINDOW
/// [`MANY_RESTARTS`]: crate::consts::MANY_RESTARTS
pub(super) fn restart_tone(restarts: i32, since_last: Option<jiff::SignedDuration>) -> Tone {
    use crate::consts::{MANY_RESTARTS, RECENT_RESTART_WINDOW};
    let recent = since_last.is_some_and(|since| {
        // A finish a little ahead of the local clock (skew) is still recent.
        since.as_secs() < RECENT_RESTART_WINDOW.as_secs() as i64
    });
    match restarts {
        0 => Tone::Neutral,
        _ if recent => Tone::Bad,
        n if n > MANY_RESTARTS => Tone::Serious,
        _ => Tone::Warning,
    }
}

/// How long before `now` the pod's most recent container restart finished: the
/// latest `lastState.terminated.finishedAt` among its restarted containers.
fn since_last_restart(status: Option<&PodStatus>, now: Timestamp) -> Option<jiff::SignedDuration> {
    status?
        .container_statuses
        .iter()
        .flatten()
        .filter(|container| container.restart_count > 0)
        .filter_map(|container| {
            container
                .last_state
                .as_ref()?
                .terminated
                .as_ref()?
                .finished_at
                .as_ref()
        })
        .map(|finished| finished.0)
        .max()
        .map(|finished| now.duration_since(finished))
}

pub(super) fn uid(pod: &Pod) -> String {
    pod.metadata.uid.clone().unwrap_or_default()
}

/// Formats an age the way `kubectl get pods` does: the single largest unit,
/// seconds up to a minute, then minutes, hours, days.
pub(crate) fn format_age(age_secs: i64) -> String {
    let age_secs = age_secs.max(0);
    if age_secs < 60 {
        format!("{age_secs}s")
    } else if age_secs < 3600 {
        format!("{}m", age_secs / 60)
    } else if age_secs < 86400 {
        format!("{}h", age_secs / 3600)
    } else {
        format!("{}d", age_secs / 86400)
    }
}

/// Projects a `Pod` into its table row, using `now` for the age column
/// (injected rather than read from the clock, so callers can test with a
/// fixed instant).
pub fn pod_row(pod: &Pod, now: Timestamp) -> PodRow {
    let status = pod.status.as_ref();
    let container_statuses = status.and_then(|s| s.container_statuses.as_ref());
    let total = container_statuses.map_or(0, |c| c.len());
    let ready_count = container_statuses.map_or(0, |c| c.iter().filter(|c| c.ready).count());
    let restarts = container_statuses.map_or(0, |c| c.iter().map(|c| c.restart_count).sum());
    let age_secs = pod
        .metadata
        .creation_timestamp
        .as_ref()
        .map(|t| now.duration_since(t.0).as_secs_f64() as i64)
        .unwrap_or_default();

    PodRow {
        name: pod.metadata.name.clone().unwrap_or_default(),
        namespace: pod.metadata.namespace.clone().unwrap_or_default(),
        ready: format!("{ready_count}/{total}"),
        status: status.and_then(|s| s.phase.clone()).unwrap_or_default(),
        restarts,
        age: format_age(age_secs),
        pod_ip: status.and_then(|s| s.pod_ip.clone()).unwrap_or_default(),
        node: pod
            .spec
            .as_ref()
            .and_then(|spec| spec.node_name.clone())
            .unwrap_or_default(),
        age_secs,
        status_tone: status_tone(status),
        ready_tone: ready_tone(ready_count, total, status.and_then(|s| s.phase.as_deref())),
        restart_tone: restart_tone(restarts, since_last_restart(status, now)),
        forwards: 0,
    }
}

/// Whether `pod` is in one of `namespaces` - every pod if the list is empty
/// (no namespace scope set). What `PodsPanel::render` filters by before
/// projecting to rows; its own name filtering runs after, over the built
/// rows' visible columns (`list-search` #189, `pods::filter`).
pub fn matches_namespaces(pod: &Pod, namespaces: &[String]) -> bool {
    namespaces.is_empty()
        || pod
            .metadata
            .namespace
            .as_ref()
            .is_some_and(|namespace| namespaces.contains(namespace))
}

#[cfg(test)]
mod tests;
