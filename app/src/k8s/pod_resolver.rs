//! Which pods a target means, and which container in one (`exec-command-library`
//! D3): the Running pods a label selector matches, in a namespace or in all of
//! them, or the pods a workload or Service selects, as `kubectl exec deploy/x`
//! resolves them. Port-forwarding to a Service resolves its pods here too.
//!
//! One list request per resolve - no watch - with the selector sent as the
//! list's `labelSelector`, so the server does the filtering on a big cluster;
//! the result is matched again here, so a server that ignores it still can't
//! hand back another app's pods. Pods come back sorted by namespace, then name.

use crate::config::exec_library::{Target, WorkloadKind};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::label_selector;
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::api::ListParams;
use kube::core::{GroupVersionKind, Selector};

/// The annotation `kubectl` reads for a pod's default container.
pub(crate) const DEFAULT_CONTAINER_ANNOTATION: &str = "kubectl.kubernetes.io/default-container";

/// The discovered kind a workload target names, for its API.
pub(crate) fn workload_kind(kind: WorkloadKind) -> DiscoveredKind {
    let (version, plural) = match kind {
        WorkloadKind::Deployment => ("v1", "deployments"),
        WorkloadKind::StatefulSet => ("v1", "statefulsets"),
        WorkloadKind::DaemonSet => ("v1", "daemonsets"),
        WorkloadKind::Service => ("v1", "services"),
    };
    DiscoveredKind {
        gvk: GroupVersionKind::gvk(kind.group(), version, kind.as_str()),
        plural: plural.into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

/// The selector the workload `name` in `namespace` selects its pods with.
pub(crate) async fn workload_selector(
    client: kube::Client,
    kind: WorkloadKind,
    namespace: &str,
    name: &str,
) -> Result<Selector, String> {
    let discovered = workload_kind(kind);
    let object =
        crate::k8s::resource::resource_actions::api_for(client, &discovered, Some(namespace))
            .get(name)
            .await
            .map_err(|error| crate::k8s::error::describe(&error))?;
    label_selector::of_object(&discovered.gvk, &object)
        .ok_or_else(|| format!("The {} {namespace}/{name} selects no pods.", kind.as_str()))
}

/// The Running pods `selector` matches in `namespace` - every namespace when
/// `None` - sorted by namespace, then name.
pub(crate) async fn running_pods(
    client: kube::Client,
    namespace: Option<&str>,
    selector: &Selector,
) -> Result<Vec<Pod>, String> {
    let api: Api<Pod> = match namespace {
        Some(namespace) => Api::namespaced(client, namespace),
        None => Api::all(client),
    };
    let params = ListParams::default().labels_from(selector);
    let listed = api
        .list(&params)
        .await
        .map_err(|error| crate::k8s::error::describe(&error))?;
    let mut pods: Vec<Pod> = listed
        .items
        .into_iter()
        .filter(|pod| label_selector::matches(selector, pod.metadata.labels.as_ref()))
        .filter(|pod| {
            namespace.is_none_or(|namespace| pod.metadata.namespace.as_deref() == Some(namespace))
        })
        .filter(is_running)
        .collect();
    pods.sort_by(|a, b| {
        (&a.metadata.namespace, &a.metadata.name).cmp(&(&b.metadata.namespace, &b.metadata.name))
    });
    Ok(pods)
}

/// Whether `pod`'s phase is Running.
pub(crate) fn is_running(pod: &Pod) -> bool {
    pod.status
        .as_ref()
        .and_then(|status| status.phase.as_deref())
        == Some("Running")
}

/// The Running pods `target` means. An any-pod target means whichever pod it
/// is run from, so it has none of its own: that is the caller's to supply.
pub(crate) async fn resolve_pods(
    client: kube::Client,
    target: &Target,
) -> Result<Vec<Pod>, String> {
    match target {
        Target::Any => Ok(Vec::new()),
        Target::Selector {
            namespace,
            selector,
        } => {
            let selector = label_selector::parse(selector).map_err(|error| error.to_string())?;
            running_pods(client, namespace.as_deref(), &selector).await
        }
        Target::Workload {
            namespace,
            workload_kind,
            name,
        } => {
            let selector =
                workload_selector(client.clone(), *workload_kind, namespace, name).await?;
            running_pods(client, Some(namespace), &selector).await
        }
    }
}

/// The container to run in on `pod`: `wanted`, when given - an error if the
/// pod has no such container - else the pod's default-container annotation,
/// else its first container.
pub(crate) fn container_for(pod: &Pod, wanted: Option<&str>) -> Result<String, String> {
    let containers: Vec<&str> = pod
        .spec
        .iter()
        .flat_map(|spec| &spec.containers)
        .map(|container| container.name.as_str())
        .collect();
    let pod_name = format!(
        "{}/{}",
        pod.metadata.namespace.as_deref().unwrap_or_default(),
        pod.metadata.name.as_deref().unwrap_or_default()
    );
    if let Some(wanted) = wanted {
        return containers
            .contains(&wanted)
            .then(|| wanted.to_string())
            .ok_or_else(|| format!("The pod {pod_name} has no container {wanted}."));
    }
    let annotated = pod
        .metadata
        .annotations
        .as_ref()
        .and_then(|annotations| annotations.get(DEFAULT_CONTAINER_ANNOTATION))
        .filter(|annotated| containers.contains(&annotated.as_str()));
    annotated
        .map(String::to_string)
        .or_else(|| containers.first().map(|first| first.to_string()))
        .ok_or_else(|| format!("The pod {pod_name} has no containers."))
}

#[cfg(test)]
mod tests;
