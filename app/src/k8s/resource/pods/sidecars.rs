//! Kubernetes' sidecar containers (#353): init containers whose
//! `restartPolicy` is `Always`. They start before the app containers and keep
//! running beside them, so a pod's readiness and restarts count them as
//! `kubectl get pods` does, and the views that list containers show them
//! apart from both the app containers and the run-once init containers.

use k8s_openapi::api::core::v1::{Container, ContainerStatus, Pod};

/// Whether `container`, one of a pod's init containers, is a sidecar.
pub(crate) fn is_sidecar(container: &Container) -> bool {
    container.restart_policy.as_deref() == Some("Always")
}

/// `pod`'s sidecars, in spec order.
fn sidecars(pod: &Pod) -> impl Iterator<Item = &Container> {
    pod.spec
        .iter()
        .flat_map(|spec| spec.init_containers.iter().flatten())
        .filter(|container| is_sidecar(container))
}

/// The containers the Logs, Shell and Port Forward pickers offer (#398) -
/// the ones that keep running: `pod`'s app containers in spec order, then its
/// sidecars, each with whether it is a sidecar. App containers come first so
/// the first offered, which a picker selects by default, stays the first app
/// container.
pub(crate) fn long_running(pod: &Pod) -> impl Iterator<Item = (&Container, bool)> {
    pod.spec
        .iter()
        .flat_map(|spec| &spec.containers)
        .map(|container| (container, false))
        .chain(sidecars(pod).map(|container| (container, true)))
}

/// [`long_running`]'s names, and which of them are sidecars - what a
/// [`super::PodSelection`] carries.
pub(crate) fn long_running_names(pod: &Pod) -> (Vec<String>, Vec<String>) {
    let containers = long_running(pod)
        .map(|(container, _)| container.name.clone())
        .collect();
    let sidecars = sidecars(pod)
        .map(|container| container.name.clone())
        .collect();
    (containers, sidecars)
}

/// How a container picker names `container`: marked when it is a sidecar.
pub(crate) fn container_label(container: &str, sidecar: bool) -> String {
    if sidecar {
        format!("{container} (sidecar)")
    } else {
        container.to_string()
    }
}

/// The statuses of `pod`'s sidecars, in spec order: its init container
/// statuses whose container is a sidecar.
pub(crate) fn sidecar_statuses(pod: &Pod) -> Vec<&ContainerStatus> {
    let statuses = pod
        .status
        .as_ref()
        .and_then(|status| status.init_container_statuses.as_deref())
        .unwrap_or_default();
    sidecars(pod)
        .filter_map(|container| statuses.iter().find(|status| status.name == container.name))
        .collect()
}

/// A pod's Ready text: `ready/total` over its app containers and sidecars,
/// as `kubectl` counts them, then how many of those are sidecars when any
/// are - `2/2 (1 sidecar)`.
pub(crate) fn ready_text(ready: usize, total: usize, sidecars: usize) -> String {
    match sidecars {
        0 => format!("{ready}/{total}"),
        1 => format!("{ready}/{total} (1 sidecar)"),
        n => format!("{ready}/{total} ({n} sidecars)"),
    }
}

#[cfg(test)]
pub(super) mod tests;
