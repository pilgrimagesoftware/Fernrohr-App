//! Status tones for kinds other than Pods (#101): how healthy a workload's
//! replicas, a Job, a volume or claim, a Namespace or a Node reads, decided
//! once here so a kind's list cell and its detail field agree. The text is
//! always shown too; the tone only colours it (`ui::style::status`).

use crate::ui::style::Tone;
use k8s_openapi::api::batch::v1::Job;
use std::fmt;

/// `ready` of `desired` replicas: all of them is good, at least half a
/// warning, fewer a serious one, none bad - and nothing wanted (scaled to
/// zero) neutral.
pub fn readiness(ready: i64, desired: i64) -> Tone {
    if desired <= 0 {
        Tone::Neutral
    } else if ready >= desired {
        Tone::Good
    } else if ready == 0 {
        Tone::Bad
    } else if ready * 2 >= desired {
        Tone::Warning
    } else {
        Tone::Serious
    }
}

/// What `kubectl get jobs` shows as a Job's status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobStatus {
    Complete,
    Failed,
    Suspended,
    Running,
}

impl JobStatus {
    /// `Complete` or `Failed` once a condition says so, `Suspended` while
    /// suspended, else `Running`.
    pub fn of(job: &Job) -> Self {
        let holds = |kind: &str| {
            job.status
                .iter()
                .flat_map(|status| status.conditions.iter().flatten())
                .any(|condition| condition.type_ == kind && condition.status == "True")
        };
        if holds("Complete") {
            Self::Complete
        } else if holds("Failed") {
            Self::Failed
        } else if job.spec.as_ref().and_then(|spec| spec.suspend) == Some(true) {
            Self::Suspended
        } else {
            Self::Running
        }
    }

    pub fn tone(self) -> Tone {
        match self {
            Self::Complete => Tone::Good,
            Self::Running => Tone::Info,
            Self::Suspended => Tone::Neutral,
            Self::Failed => Tone::Bad,
        }
    }
}

impl fmt::Display for JobStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Complete => "Complete",
            Self::Failed => "Failed",
            Self::Suspended => "Suspended",
            Self::Running => "Running",
        })
    }
}

/// A PersistentVolumeClaim's phase: bound is good, pending under way, lost
/// bad.
pub fn claim_phase(phase: &str) -> Tone {
    match phase {
        "Bound" => Tone::Good,
        "Pending" => Tone::Info,
        "Lost" => Tone::Bad,
        _ => Tone::Neutral,
    }
}

/// A PersistentVolume's phase: available or bound is good, pending under way,
/// released (its claim gone, not yet reclaimed) a warning, failed bad.
pub fn volume_phase(phase: &str) -> Tone {
    match phase {
        "Available" | "Bound" => Tone::Good,
        "Pending" => Tone::Info,
        "Released" => Tone::Warning,
        "Failed" => Tone::Bad,
        _ => Tone::Neutral,
    }
}

/// A Namespace's phase: active is good, terminating a warning.
pub fn namespace_phase(phase: &str) -> Tone {
    match phase {
        "Active" => Tone::Good,
        "Terminating" => Tone::Warning,
        _ => Tone::Neutral,
    }
}

/// A Node by its `Ready` condition's status and whether it is cordoned: ready
/// is good, ready but cordoned a warning, not ready (`False`, or `Unknown`
/// once the kubelet stops reporting) bad, and no condition yet neutral.
pub fn node(ready: Option<&str>, cordoned: bool) -> Tone {
    match ready {
        Some("True") if cordoned => Tone::Warning,
        Some("True") => Tone::Good,
        Some(_) => Tone::Bad,
        None => Tone::Neutral,
    }
}

#[cfg(test)]
mod tests;
