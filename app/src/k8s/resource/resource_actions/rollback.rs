//! Rolling a workload back to its previous revision, as `kubectl rollout
//! undo` does, in two steps so a caller can show what it will do first:
//!
//! - [`plan_rollback`] reads the workload's history and finds the current and
//!   previous revisions - for a Deployment its ReplicaSets'
//!   `deployment.kubernetes.io/revision`, for a StatefulSet or DaemonSet its
//!   ControllerRevisions - and builds the patch that restores the previous
//!   pod template.
//! - [`rollback`] sends that patch. It carries the `resourceVersion` the plan
//!   was read at, so if the workload changed in between the API refuses it
//!   with a conflict rather than rolling back from a state nobody saw.

use super::{ActionError, api_for, patch_params};
use crate::k8s::cluster::discovery::DiscoveredKind;
use k8s_openapi::api::apps::v1::{ControllerRevision, ReplicaSet};
use kube::Api;
use kube::api::{ListParams, Patch};
use serde_json::{Value, json};

/// A Deployment's (and its ReplicaSets') revision annotation.
const REVISION_ANNOTATION: &str = "deployment.kubernetes.io/revision";
/// The label the Deployment controller adds to each ReplicaSet's template;
/// a restored template must not carry it.
const POD_TEMPLATE_HASH: &str = "pod-template-hash";

/// What [`rollback`] will do: from which revision to which, and the patch.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RollbackPlan {
    pub(crate) current_revision: i64,
    pub(crate) target_revision: i64,
    patch: Value,
}

/// Finds `name`'s previous revision. Fails with a precondition when it has
/// none - a workload never updated has nothing to go back to.
pub(crate) async fn plan_rollback(
    client: kube::Client,
    kind: &DiscoveredKind,
    namespace: &str,
    name: &str,
) -> Result<RollbackPlan, ActionError> {
    let workload = api_for(client.clone(), kind, Some(namespace))
        .get(name)
        .await?;
    let uid = workload.metadata.uid.clone().unwrap_or_default();
    let resource_version = workload.metadata.resource_version.clone();
    let revisions = if kind.gvk.kind == "Deployment" {
        deployment_revisions(client, namespace, &uid).await?
    } else {
        controller_revisions(client, namespace, &uid).await?
    };
    let current_revision = if kind.gvk.kind == "Deployment" {
        workload
            .metadata
            .annotations
            .as_ref()
            .and_then(|annotations| annotations.get(REVISION_ANNOTATION))
            .and_then(|revision| revision.parse().ok())
            .or_else(|| revisions.iter().map(|(revision, _)| *revision).max())
    } else {
        revisions.iter().map(|(revision, _)| *revision).max()
    };
    let Some(current_revision) = current_revision else {
        return Err(no_history(kind, name));
    };
    let Some((target_revision, template_patch)) = revisions
        .into_iter()
        .filter(|(revision, _)| *revision < current_revision)
        .max_by_key(|(revision, _)| *revision)
    else {
        return Err(no_history(kind, name));
    };
    Ok(RollbackPlan {
        current_revision,
        target_revision,
        patch: with_resource_version(template_patch, resource_version),
    })
}

/// Sends `plan`'s patch.
pub(crate) async fn rollback(
    client: kube::Client,
    kind: &DiscoveredKind,
    namespace: &str,
    name: &str,
    plan: &RollbackPlan,
) -> Result<(), ActionError> {
    api_for(client, kind, Some(namespace))
        .patch(name, &patch_params(), &Patch::Strategic(&plan.patch))
        .await?;
    Ok(())
}

fn no_history(kind: &DiscoveredKind, name: &str) -> ActionError {
    ActionError::Precondition(format!(
        "{} {name:?} has no earlier revision to roll back to",
        kind.gvk.kind
    ))
}

/// A Deployment's revisions: each owned ReplicaSet's number, and the patch
/// that puts its template back - replacing the template whole, as kubectl's
/// own rollback does, so nothing the newer template added survives.
async fn deployment_revisions(
    client: kube::Client,
    namespace: &str,
    owner_uid: &str,
) -> Result<Vec<(i64, Value)>, ActionError> {
    let sets = Api::<ReplicaSet>::namespaced(client, namespace)
        .list(&ListParams::default())
        .await?;
    Ok(sets
        .items
        .into_iter()
        .filter(|set| owned_by(&set.metadata, owner_uid))
        .filter_map(|set| {
            let revision = set
                .metadata
                .annotations
                .as_ref()?
                .get(REVISION_ANNOTATION)?
                .parse()
                .ok()?;
            let mut template = serde_json::to_value(set.spec?.template?).ok()?;
            if let Some(labels) = template["metadata"]["labels"].as_object_mut() {
                labels.remove(POD_TEMPLATE_HASH);
            }
            template["$patch"] = json!("replace");
            Some((revision, json!({ "spec": { "template": template } })))
        })
        .collect())
}

/// A StatefulSet's or DaemonSet's revisions: each owned ControllerRevision's
/// number and its `data`, which is already the patch that restores it.
async fn controller_revisions(
    client: kube::Client,
    namespace: &str,
    owner_uid: &str,
) -> Result<Vec<(i64, Value)>, ActionError> {
    let revisions = Api::<ControllerRevision>::namespaced(client, namespace)
        .list(&ListParams::default())
        .await?;
    Ok(revisions
        .items
        .into_iter()
        .filter(|revision| owned_by(&revision.metadata, owner_uid))
        .filter_map(|revision| Some((revision.revision, revision.data?.0)))
        .collect())
}

fn owned_by(
    metadata: &k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta,
    owner_uid: &str,
) -> bool {
    !owner_uid.is_empty()
        && metadata
            .owner_references
            .iter()
            .flatten()
            .any(|owner| owner.uid == owner_uid)
}

/// `patch`, made conditional on the workload still being at
/// `resource_version`.
fn with_resource_version(mut patch: Value, resource_version: Option<String>) -> Value {
    if let (Some(object), Some(version)) = (patch.as_object_mut(), resource_version) {
        object.insert(
            "metadata".to_string(),
            json!({ "resourceVersion": version }),
        );
    }
    patch
}
