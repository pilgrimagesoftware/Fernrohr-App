//! A pod at a glance (`pod-quick-look`): the handful of fields the Pods table's
//! quick look shows, projected with the same helpers the detail panel's fields
//! and the table's row use, so the three never disagree about a pod.
//!
//! Owns the projection only; the popover that draws it is the Pods panel's
//! (`pods::quick_look`).

use super::format::summarize_containers;
use super::references::owners;
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::pods::pod_row;
use crate::ui::detail::BadgeTone;
use crate::ui::style::Tone;
use jiff::Timestamp;
use k8s_openapi::api::core::v1::Pod;

/// What the quick look shows about one pod.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Glance {
    pub name: String,
    pub namespace: String,
    /// What the Pods table's Status column says - the phase - in the tone it
    /// gives it, which is bad when a container is stuck.
    pub status: String,
    pub status_tone: Tone,
    /// Ready containers of all, `1/2`.
    pub ready: String,
    pub ready_tone: Tone,
    pub restarts: i32,
    pub age: String,
    pub node: String,
    pub pod_ip: String,
    /// The pod's owners, each followable as a link where its kind is known.
    pub owners: Vec<ObjectRef>,
    pub containers: Vec<GlanceContainer>,
}

/// One container's line: its image and state.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GlanceContainer {
    pub name: String,
    pub image: String,
    pub state: String,
    pub state_tone: BadgeTone,
}

/// `pod` at a glance, ages measured at `now`.
pub(crate) fn glance(pod: &Pod, now: Timestamp) -> Glance {
    let row = pod_row(pod, now);
    let containers = pod
        .spec
        .as_ref()
        .map(|spec| {
            let statuses = pod
                .status
                .as_ref()
                .and_then(|status| status.container_statuses.as_deref())
                .unwrap_or_default();
            summarize_containers(&spec.containers, statuses, &row.namespace)
        })
        .unwrap_or_default()
        .into_iter()
        .map(|summary| GlanceContainer {
            name: summary.name,
            image: summary.image,
            state: summary.state,
            state_tone: summary.state_tone,
        })
        .collect();
    Glance {
        owners: owners(pod),
        containers,
        name: row.name,
        namespace: row.namespace,
        status: row.status,
        status_tone: row.status_tone,
        ready: row.ready,
        ready_tone: row.ready_tone,
        restarts: row.restarts,
        age: row.age,
        node: row.node,
        pod_ip: row.pod_ip,
    }
}

#[cfg(test)]
mod tests;
