//! Formatting helpers for the projection: one API sub-object (a container, a
//! volume, a toleration, an event) in, the display shape the model wants out.

use super::model::{ContainerSummary, ManagedFieldEntry};
use crate::k8s::resource::pods::format_age;
use crate::ui::detail::BadgeTone;
use k8s_openapi::api::core::v1::Pod;

/// One `managedFields` entry, with its `fieldsV1` ownership tree
/// pretty-printed - or a placeholder noting there was none to print, since an
/// entry missing a manager name still names an operation worth showing rather
/// than being dropped.
pub(super) fn managed_field_entry(
    entry: &k8s_openapi::apimachinery::pkg::apis::meta::v1::ManagedFieldsEntry,
) -> ManagedFieldEntry {
    let manager = non_empty(&entry.manager)
        .map(str::to_string)
        .unwrap_or_else(|| "unknown manager".to_string());
    let operation = non_empty(&entry.operation)
        .map(str::to_string)
        .unwrap_or_else(|| "unknown operation".to_string());
    let fields_json = entry
        .fields_v1
        .as_ref()
        .and_then(|fields| serde_json::to_string_pretty(&fields.0).ok())
        .unwrap_or_else(|| "(no field ownership recorded)".to_string());
    ManagedFieldEntry {
        manager,
        operation,
        time: entry.time.as_ref().map(|time| time.0),
        fields_json,
    }
}

/// Joins `containers` with their matching entry in `statuses` by name - the
/// two live on separate parts of the `Pod` object, and a container with no
/// status yet (still scheduling) still gets a row, just without
/// ready/restart/state data. `spec.containers` pair with
/// `status.containerStatuses` and `spec.initContainers` with
/// `status.initContainerStatuses`: reading an init container's state from the
/// app containers' statuses showed it with someone else's state, or none.
pub(super) fn summarize_containers(
    containers: &[k8s_openapi::api::core::v1::Container],
    statuses: &[k8s_openapi::api::core::v1::ContainerStatus],
    namespace: &str,
) -> Vec<ContainerSummary> {
    containers
        .iter()
        .map(|container| {
            let matching = statuses.iter().find(|s| s.name == container.name);
            let ports = container
                .ports
                .iter()
                .flatten()
                .map(|port| match non_empty(&port.protocol) {
                    Some(protocol) => format!("{}/{protocol}", port.container_port),
                    None => port.container_port.to_string(),
                })
                .collect();
            let (requests, limits) = container
                .resources
                .as_ref()
                .map(|resources| {
                    (
                        format_quantities(&resources.requests),
                        format_quantities(&resources.limits),
                    )
                })
                .unwrap_or_default();
            ContainerSummary {
                name: container.name.clone(),
                image: container.image.clone().unwrap_or_default(),
                ready: matching.map(|status| status.ready),
                ready_tone: container_ready_tone(matching),
                restart_count: matching.map(|status| status.restart_count).unwrap_or(0),
                state: matching
                    .and_then(|status| status.state.as_ref())
                    .map(format_container_state)
                    .unwrap_or_else(|| "Waiting".to_string()),
                state_message: matching
                    .and_then(|status| status.state.as_ref())
                    .and_then(container_state_message),
                state_tone: matching
                    .and_then(|status| status.state.as_ref())
                    .map_or(BadgeTone::Info, container_state_tone),
                ports,
                requests,
                limits,
                env_sources: super::references::env_sources(container, namespace),
                detail: super::container_detail::container_detail(container),
            }
        })
        .collect()
}

pub(super) fn format_quantities(
    quantities: &Option<
        std::collections::BTreeMap<String, k8s_openapi::apimachinery::pkg::api::resource::Quantity>,
    >,
) -> Vec<String> {
    quantities
        .iter()
        .flatten()
        .map(|(resource, quantity)| format!("{resource}={}", quantity.0))
        .collect()
}

/// A container's readiness as a tone. Not ready is a warning, except for one
/// that ran to completion - exit 0, as a Job's or a `Succeeded` pod's
/// containers do - which needn't be ready (#120).
pub(super) fn container_ready_tone(
    status: Option<&k8s_openapi::api::core::v1::ContainerStatus>,
) -> BadgeTone {
    let Some(status) = status else {
        return BadgeTone::Unknown;
    };
    let completed = status
        .state
        .as_ref()
        .and_then(|state| state.terminated.as_ref())
        .is_some_and(|terminated| terminated.exit_code == 0);
    match (status.ready, completed) {
        (true, _) => BadgeTone::Good,
        (false, true) => BadgeTone::Unknown,
        (false, false) => BadgeTone::Warning,
    }
}

/// The one human string a `ContainerState` union renders as - a reason when
/// the cluster gave one (`ImagePullBackOff`, `Completed`), the bare state
/// name otherwise, and a terminated container's exit code.
pub(super) fn format_container_state(state: &k8s_openapi::api::core::v1::ContainerState) -> String {
    if let Some(running) = &state.running {
        let _ = running;
        return "Running".to_string();
    }
    if let Some(waiting) = &state.waiting {
        return match non_empty(&waiting.reason) {
            Some(reason) => format!("Waiting: {reason}"),
            None => "Waiting".to_string(),
        };
    }
    if let Some(terminated) = &state.terminated {
        let exit = terminated.exit_code;
        return match non_empty(&terminated.reason) {
            Some(reason) => format!("Terminated: {reason} (exit {exit})"),
            None => format!("Terminated (exit {exit})"),
        };
    }
    "Unknown".to_string()
}

/// The message a waiting or terminated container's state carries - why it
/// waits, or what it said on the way out - when the cluster gave one.
pub(super) fn container_state_message(
    state: &k8s_openapi::api::core::v1::ContainerState,
) -> Option<String> {
    let message = state
        .waiting
        .as_ref()
        .and_then(|waiting| waiting.message.clone())
        .or_else(|| {
            state
                .terminated
                .as_ref()
                .and_then(|terminated| terminated.message.clone())
        })?;
    let message = message.trim();
    (!message.is_empty()).then(|| message.to_string())
}

/// How healthy a container's state is (`resource-detail-ui-improvements`):
/// running, or terminated cleanly (`Completed`), is success; waiting is info -
/// under way - unless its reason is one the Pods table's Status colour also
/// counts as stuck (`CrashLoopBackOff`, `ImagePullBackOff`, ...), which is
/// danger, as is a non-zero exit.
pub(super) fn container_state_tone(
    state: &k8s_openapi::api::core::v1::ContainerState,
) -> BadgeTone {
    use crate::k8s::resource::pods::BAD_WAITING_REASONS;
    if state.running.is_some() {
        return BadgeTone::Good;
    }
    if let Some(waiting) = &state.waiting {
        let stuck = waiting
            .reason
            .as_deref()
            .is_some_and(|reason| BAD_WAITING_REASONS.contains(&reason));
        return if stuck {
            BadgeTone::Bad
        } else {
            BadgeTone::Info
        };
    }
    if let Some(terminated) = &state.terminated {
        return if terminated.exit_code == 0 {
            BadgeTone::Good
        } else {
            BadgeTone::Bad
        };
    }
    BadgeTone::Unknown
}

/// One toleration, in kubectl's key/operator/value/effect shape. Absent pieces
/// are left out rather than printed as `None`.
pub(super) fn format_toleration(toleration: &k8s_openapi::api::core::v1::Toleration) -> String {
    let operator = toleration.operator.as_deref().unwrap_or("Equal");
    let mut parts = Vec::new();
    match (non_empty(&toleration.key), non_empty(&toleration.value)) {
        // `Exists` matches on the key alone, so a value would be noise.
        (Some(key), _) if operator == "Exists" => parts.push(key.to_string()),
        (Some(key), Some(value)) => parts.push(format!("{key}={value}")),
        (Some(key), None) => parts.push(key.to_string()),
        (None, _) => parts.push(format!("<any> ({operator})")),
    }
    if let Some(effect) = non_empty(&toleration.effect) {
        parts.push(effect.to_string());
    }
    if let Some(seconds) = toleration.toleration_seconds {
        parts.push(format!("for {}", format_age(seconds)));
    }
    parts.join(": ")
}

pub(super) enum IpSource {
    Host,
    Pod,
}

/// The pod's host or pod IPs, joined for display. Falls back to the singular
/// `host_ip`/`pod_ip` field when the plural one is absent, which is how older
/// clusters report a single address.
pub(super) fn ips_of(pod: &Pod, source: IpSource) -> Option<String> {
    let status = pod.status.as_ref()?;
    let addresses: Vec<String> = match source {
        IpSource::Host => {
            let plural: Vec<String> = status
                .host_ips
                .iter()
                .flatten()
                .map(|ip| ip.ip.clone())
                .collect();
            if plural.is_empty() {
                status.host_ip.iter().cloned().collect()
            } else {
                plural
            }
        }
        IpSource::Pod => {
            let plural: Vec<String> = status
                .pod_ips
                .iter()
                .flatten()
                .map(|ip| ip.ip.clone())
                .collect();
            if plural.is_empty() {
                status.pod_ip.iter().cloned().collect()
            } else {
                plural
            }
        }
    };
    if addresses.is_empty() {
        None
    } else {
        Some(addresses.join(", "))
    }
}

pub(super) fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|value| !value.is_empty())
}

pub(super) fn non_empty_map(
    map: &Option<std::collections::BTreeMap<String, String>>,
) -> Option<&std::collections::BTreeMap<String, String>> {
    map.as_ref().filter(|map| !map.is_empty())
}
