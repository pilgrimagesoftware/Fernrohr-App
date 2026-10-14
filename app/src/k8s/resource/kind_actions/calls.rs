//! The kind-specific actions' cluster calls. Each mutation of a listed object
//! carries its UID ([`resource_actions::with_uid`]); a taint change, which
//! writes the whole `spec.taints` list, carries the `resourceVersion` it read
//! too, so it can't drop a taint someone added since.

use super::facts::{ObjectFacts, Taint};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::bulk::failure;
use crate::k8s::resource::resource_actions::{
    self, ActionError, ActionFailure, api_for, patch_params, with_uid,
};
use crate::k8s::resource::secret_value::SecretValue;
use kube::api::Patch;
use serde_json::{Value, json};

/// Trigger Now: a Job from CronJob `name`'s job template, as `kubectl create
/// job --from=cronjob/<name>` makes - the agent tools' own call - only from the
/// CronJob with `uid`. The Job's name.
pub(crate) async fn trigger_now(
    client: kube::Client,
    namespace: &str,
    name: &str,
    uid: Option<&str>,
) -> Result<String, ActionFailure> {
    let suffix = resource_actions::job_suffix().ok_or_else(|| ActionFailure {
        message: "No random name suffix could be made for the Job.".to_string(),
        detail: String::new(),
    })?;
    let plan = resource_actions::plan_trigger(client.clone(), namespace, name, &suffix, uid)
        .await
        .map_err(failure)?;
    resource_actions::trigger(client, namespace, &plan)
        .await
        .map_err(failure)?;
    Ok(plan.job_name)
}

/// Suspend or Resume a Job or CronJob.
pub(crate) async fn set_suspended(
    client: kube::Client,
    kind: &DiscoveredKind,
    namespace: &str,
    name: &str,
    suspended: bool,
    uid: Option<&str>,
) -> Result<(), ActionFailure> {
    resource_actions::set_suspended(client, kind, namespace, name, suspended, uid)
        .await
        .map_err(refused)
}

/// Pause or Resume a Deployment's rollout.
pub(crate) async fn set_paused(
    client: kube::Client,
    kind: &DiscoveredKind,
    namespace: &str,
    name: &str,
    paused: bool,
    uid: Option<&str>,
) -> Result<(), ActionFailure> {
    resource_actions::set_rollout_paused(client, kind, namespace, name, paused, uid)
        .await
        .map_err(refused)
}

/// Expand: a claim's requested storage set to `size`. Whether its StorageClass
/// allows it is the cluster's to say; its refusal is reported as it gives it.
pub(crate) async fn expand(
    client: kube::Client,
    kind: &DiscoveredKind,
    namespace: &str,
    name: &str,
    size: &str,
    uid: Option<&str>,
) -> Result<(), ActionFailure> {
    let patch = with_uid(
        json!({ "spec": { "resources": { "requests": { "storage": size } } } }),
        uid,
    );
    merge(client, kind, Some(namespace), name, &patch).await
}

/// Add Taint: `taint` added to Node `name`'s taints as they are when this
/// runs - read now, not when the dialog opened, so a busy node's other changes
/// don't refuse it - pinned to that read's version, so a change between the
/// read and the write is still caught. The node must still be the one with
/// `uid`, and mustn't already have `taint`'s key and effect.
pub(crate) async fn add_taint(
    client: kube::Client,
    kind: &DiscoveredKind,
    name: &str,
    uid: Option<&str>,
    taint: &Taint,
) -> Result<(), ActionFailure> {
    let facts = read_node(client.clone(), kind, name, uid).await?;
    if facts.taints.iter().any(|existing| existing.same_as(taint)) {
        return Err(ActionFailure {
            message: format!("The node already has a {} taint.", taint.label()),
            detail: String::new(),
        });
    }
    let mut taints: Vec<Value> = facts.taints.iter().map(|taint| taint.raw.clone()).collect();
    taints.push(taint.raw.clone());
    merge(client, kind, None, name, &taints_patch(&facts, taints)).await
}

/// Remove Taint: Node `name`'s taints, as they are when this runs, without
/// `taint` - as [`add_taint`] reads and pins them.
pub(crate) async fn remove_taint(
    client: kube::Client,
    kind: &DiscoveredKind,
    name: &str,
    uid: Option<&str>,
    taint: &Taint,
) -> Result<(), ActionFailure> {
    let facts = read_node(client.clone(), kind, name, uid).await?;
    if !facts.taints.iter().any(|existing| existing.same_as(taint)) {
        // Someone else removed it: what was asked is already so.
        return Ok(());
    }
    let taints: Vec<Value> = facts
        .taints
        .iter()
        .filter(|kept| !kept.same_as(taint))
        .map(|kept| kept.raw.clone())
        .collect();
    merge(client, kind, None, name, &taints_patch(&facts, taints)).await
}

/// The node's facts now, if it is still the one with `uid`.
async fn read_node(
    client: kube::Client,
    kind: &DiscoveredKind,
    name: &str,
    uid: Option<&str>,
) -> Result<ObjectFacts, ActionFailure> {
    let node = api_for(client, kind, None)
        .get(name)
        .await
        .map_err(|error| ActionFailure::from_kube(&error))?;
    if uid.is_some_and(|uid| node.metadata.uid.as_deref() != Some(uid)) {
        return Err(stale(String::new()));
    }
    Ok(ObjectFacts::of(&node))
}

/// The whole `spec.taints` list, only for the object and version `facts` read.
pub(super) fn taints_patch(facts: &ObjectFacts, taints: Vec<Value>) -> Value {
    let mut patch = with_uid(
        json!({ "spec": { "taints": taints } }),
        facts.uid.as_deref(),
    );
    if let Some(version) = &facts.resource_version {
        patch["metadata"]["resourceVersion"] = Value::String(version.clone());
    }
    patch
}

async fn merge(
    client: kube::Client,
    kind: &DiscoveredKind,
    namespace: Option<&str>,
    name: &str,
    patch: &Value,
) -> Result<(), ActionFailure> {
    api_for(client, kind, namespace)
        .patch(name, &patch_params(), &Patch::Merge(patch))
        .await
        .map(|_| ())
        .map_err(|error| refused(ActionError::Kube(error)))
}

/// A refusal as the panel shows it. A conflict on the UID or version a patch
/// carried says what it means; any other refusal is the cluster's own words.
fn refused(error: ActionError) -> ActionFailure {
    let conflict = matches!(&error, ActionError::Kube(kube::Error::Api(status))
        if status.code == 409 || (status.code == 422 && status.message.contains("uid")));
    let failure = failure(error);
    if conflict {
        stale(failure.detail)
    } else {
        failure
    }
}

/// The object changed, or was recreated, since it was read.
fn stale(detail: String) -> ActionFailure {
    ActionFailure {
        message: resource_actions::RECREATED.to_string(),
        detail,
    }
}

/// The TokenRequest Create Token sends: the API server's default audience and
/// lifetime.
pub(super) fn token_request() -> Value {
    json!({ "apiVersion": "authentication.k8s.io/v1", "kind": "TokenRequest", "spec": {} })
}

/// Create Token: a new token for ServiceAccount `name`, held as a
/// [`SecretValue`], which never prints, serializes or logs its value.
///
/// Sent by hand rather than through a kube helper, so the request body is the
/// one this module builds - and so no error ever carries the answer, which
/// holds the token: an answer without one says only that.
pub(crate) async fn create_token(
    client: kube::Client,
    namespace: &str,
    name: &str,
) -> Result<SecretValue, ActionFailure> {
    let request = http::Request::post(format!(
        "/api/v1/namespaces/{namespace}/serviceaccounts/{name}/token"
    ))
    .header(http::header::CONTENT_TYPE, "application/json")
    .body(token_request().to_string().into_bytes())
    .map_err(|_| no_token("The token request couldn't be built."))?;
    let answer = client
        .request_text(request)
        .await
        .map_err(|error| ActionFailure::from_kube(&error))?;
    let answer: Value = serde_json::from_str(&answer)
        .map_err(|_| no_token("The cluster's answer wasn't a TokenRequest."))?;
    answer["status"]["token"]
        .as_str()
        .filter(|token| !token.is_empty())
        .map(|token| SecretValue::new(token.as_bytes().to_vec()))
        .ok_or_else(|| no_token("The cluster's answer held no token."))
}

fn no_token(message: &str) -> ActionFailure {
    ActionFailure {
        message: message.to_string(),
        detail: String::new(),
    }
}
