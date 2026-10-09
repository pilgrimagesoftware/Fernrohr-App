//! Scaling, restarting and pausing a workload - each one field, set by a patch
//! that carries nothing else:
//!
//! - [`scale`]: `spec.replicas` through the `scale` subresource, as
//!   `kubectl scale` does, so the workload's own spec is never written.
//! - [`restart`]: the pod template's `kubectl.kubernetes.io/restartedAt`
//!   annotation, as `kubectl rollout restart` does; the controller rolls the
//!   pods because the template changed.
//! - [`set_rollout_paused`]: a Deployment's `spec.paused`.

use super::{ActionError, api_for, patch_params};
use crate::k8s::cluster::discovery::DiscoveredKind;
use kube::api::Patch;
use serde_json::json;

/// The annotation a rollout restart sets, as kubectl names it.
const RESTARTED_AT: &str = "kubectl.kubernetes.io/restartedAt";

/// `name`'s replica count, from its `scale` subresource.
pub(crate) async fn current_replicas(
    client: kube::Client,
    kind: &DiscoveredKind,
    namespace: &str,
    name: &str,
) -> Result<i32, ActionError> {
    let scale = api_for(client, kind, Some(namespace))
        .get_scale(name)
        .await?;
    Ok(scale
        .spec
        .and_then(|spec| spec.replicas)
        .unwrap_or_default())
}

/// Sets `name`'s replica count, and returns the count the API reports back.
pub(crate) async fn scale(
    client: kube::Client,
    kind: &DiscoveredKind,
    namespace: &str,
    name: &str,
    replicas: i32,
) -> Result<i32, ActionError> {
    let patch = json!({ "spec": { "replicas": replicas } });
    let scale = api_for(client, kind, Some(namespace))
        .patch_scale(name, &patch_params(), &Patch::Merge(&patch))
        .await?;
    Ok(scale
        .spec
        .and_then(|spec| spec.replicas)
        .unwrap_or(replicas))
}

/// Restarts `name`'s rollout by stamping its pod template with `at`.
pub(crate) async fn restart(
    client: kube::Client,
    kind: &DiscoveredKind,
    namespace: &str,
    name: &str,
    at: jiff::Timestamp,
) -> Result<(), ActionError> {
    api_for(client, kind, Some(namespace))
        .patch(name, &patch_params(), &Patch::Merge(&restart_patch(at)))
        .await?;
    Ok(())
}

/// The restart patch: only the template's one annotation, stamped as
/// `kubectl` stamps it - RFC 3339, to the second.
pub(super) fn restart_patch(at: jiff::Timestamp) -> serde_json::Value {
    let stamp = at.strftime("%Y-%m-%dT%H:%M:%SZ").to_string();
    json!({
        "spec": { "template": { "metadata": { "annotations": { RESTARTED_AT: stamp } } } }
    })
}

/// Whether Deployment `name`'s rollout is paused now.
pub(crate) async fn deployment_paused(
    client: kube::Client,
    kind: &DiscoveredKind,
    namespace: &str,
    name: &str,
) -> Result<bool, ActionError> {
    let deployment = api_for(client, kind, Some(namespace)).get(name).await?;
    Ok(deployment.data["spec"]["paused"].as_bool().unwrap_or(false))
}

/// Pauses or resumes Deployment `name`'s rollout.
pub(crate) async fn set_rollout_paused(
    client: kube::Client,
    kind: &DiscoveredKind,
    namespace: &str,
    name: &str,
    paused: bool,
) -> Result<(), ActionError> {
    let patch = json!({ "spec": { "paused": paused } });
    api_for(client, kind, Some(namespace))
        .patch(name, &patch_params(), &Patch::Merge(&patch))
        .await?;
    Ok(())
}
