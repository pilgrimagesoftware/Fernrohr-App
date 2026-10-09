//! Acting on a resource rather than viewing it (`k9s-remaining-keybindings`):
//! the cluster calls behind the delete and force-kill commands. One helper per
//! verb, kind-agnostic through the discovered kind's `ApiResource`, so any
//! resource kind that registers the commands shares the request.
//!
//! Owns only the request and its outcome; which panel offers a command, and
//! whether it confirms first, is the panel's. Editing as YAML is [`apply`]'s.
//!
//! The workload, rollout, CronJob and ConfigMap actions (`workloads`,
//! `rollback`, `cronjobs`, `configmaps`) each build their request from typed
//! fields, so nothing outside the action's own fields can change. They are
//! shared by every caller - `agent-mcp`'s action tools first - and report
//! [`ActionError`], which keeps the `kube::Error` for the caller to render.

use crate::k8s::cluster::discovery::DiscoveredKind;
use kube::Api;
use kube::api::{ApiResource, DeleteParams, DynamicObject};

mod apply;
mod configmaps;
mod cronjobs;
mod rollback;
mod workloads;
pub(crate) use apply::{apply, edit_text, parse_manifest};
pub(crate) use configmaps::{configmap_value, set_configmap_value};
pub(crate) use cronjobs::{cronjob_suspended, plan_trigger, set_cronjob_suspended, trigger};
pub(crate) use rollback::{plan_rollback, rollback};
pub(crate) use workloads::{
    current_replicas, deployment_paused, restart, scale, set_rollout_paused,
};

/// Why an action failed: readable prose, and the full technical rendering kept
/// alongside it (`k8s::error::{describe, detail}`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ActionFailure {
    pub(crate) message: String,
    pub(crate) detail: String,
}

impl ActionFailure {
    pub(crate) fn from_kube(error: &kube::Error) -> Self {
        Self {
            message: crate::k8s::error::describe(error),
            detail: crate::k8s::error::detail(error),
        }
    }
}

/// Why a shared action failed.
#[derive(Debug)]
pub(crate) enum ActionError {
    /// The cluster refused the request, or couldn't be reached.
    Kube(kube::Error),
    /// The object isn't in a state the action applies to - no earlier
    /// revision to roll back to, say. The message is this module's own.
    Precondition(String),
}

impl From<kube::Error> for ActionError {
    fn from(error: kube::Error) -> Self {
        Self::Kube(error)
    }
}

/// Patch options for every action's patch: the app's field manager.
pub(super) fn patch_params() -> kube::api::PatchParams {
    kube::api::PatchParams {
        field_manager: Some(apply::FIELD_MANAGER.to_string()),
        ..kube::api::PatchParams::default()
    }
}

/// The API for `kind`'s objects in `namespace` - all namespaces for a
/// cluster-scoped kind or a missing namespace.
pub(crate) fn api_for(
    client: kube::Client,
    kind: &DiscoveredKind,
    namespace: Option<&str>,
) -> Api<DynamicObject> {
    let resource = ApiResource::from_gvk_with_plural(&kind.gvk, &kind.plural);
    match (namespace, kind.namespaced) {
        (Some(namespace), true) => Api::namespaced_with(client, namespace, &resource),
        _ => Api::all_with(client, &resource),
    }
}

/// The delete options for a normal delete, or - `force` - a force kill: zero
/// grace period, so the kubelet stops the containers at once.
pub(crate) fn delete_params(force: bool) -> DeleteParams {
    if force {
        DeleteParams {
            grace_period_seconds: Some(0),
            ..DeleteParams::default()
        }
    } else {
        DeleteParams::default()
    }
}

/// Deletes `name` of `kind` in `namespace`. A 404 is success: the object is
/// already gone, which is what was asked for.
pub(crate) async fn delete(
    client: kube::Client,
    kind: &DiscoveredKind,
    name: &str,
    namespace: Option<&str>,
    force: bool,
) -> Result<(), ActionFailure> {
    delete_object(client, kind, name, namespace, force)
        .await
        .map_err(|error| ActionFailure::from_kube(&error))
}

/// [`delete`], keeping kube's error for a caller that renders it its own way.
pub(crate) async fn delete_object(
    client: kube::Client,
    kind: &DiscoveredKind,
    name: &str,
    namespace: Option<&str>,
    force: bool,
) -> Result<(), kube::Error> {
    match api_for(client, kind, namespace)
        .delete(name, &delete_params(force))
        .await
    {
        Ok(_) => Ok(()),
        Err(kube::Error::Api(status)) if status.code == 404 => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests;
