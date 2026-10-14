//! Cordon, Uncordon and Drain, as `kubectl` does them:
//!
//! - [`set_unschedulable`]: a Node's `spec.unschedulable` set, or cleared.
//! - [`drain`]: cordon, then evict each of the node's pods through the
//!   eviction subresource, which is what makes a PodDisruptionBudget hold. Like
//!   `kubectl drain` it leaves DaemonSet pods (their controller would put them
//!   straight back) and mirror pods (the kubelet owns them) alone. It doesn't
//!   evict what `kubectl drain` refuses to without a flag: a pod no controller
//!   manages (`--force`), since nothing would recreate it, and a pod with an
//!   `emptyDir` volume (`--delete-emptydir-data`), whose data would be lost.
//!   Each is reported as not evicted rather than silently left running. Every
//!   eviction carries the listed pod's UID, so a pod of the same name created
//!   since the list isn't the one evicted.

use super::{BulkTarget, ObjectOutcome};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::resource_actions::{ActionFailure, api_for, patch_params};
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::api::{ListParams, Patch};
use serde_json::{Value, json};
use std::time::Duration;

/// The annotation the kubelet puts on a static pod's API mirror.
const MIRROR_ANNOTATION: &str = "kubernetes.io/config.mirror";

/// The cordon patch, or - `false` - the uncordon patch, which clears the field
/// as `kubectl uncordon` does.
pub(super) fn unschedulable_patch(unschedulable: bool) -> Value {
    let value = if unschedulable {
        Value::Bool(true)
    } else {
        Value::Null
    };
    json!({ "spec": { "unschedulable": value } })
}

/// Cordons the node `name`, or uncordons it.
pub(super) async fn set_unschedulable(
    client: kube::Client,
    kind: &DiscoveredKind,
    name: &str,
    unschedulable: bool,
) -> Result<(), ActionFailure> {
    api_for(client, kind, None)
        .patch(
            name,
            &patch_params(),
            &Patch::Merge(&unschedulable_patch(unschedulable)),
        )
        .await
        .map(|_| ())
        .map_err(|error| ActionFailure::from_kube(&error))
}

/// What drain does with one of the node's pods.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PodPlan {
    Evict,
    /// Left alone, as `kubectl drain` leaves it: a DaemonSet's or a mirror pod.
    Skip,
    /// Not evicted, and reported: no controller would recreate it.
    Unmanaged,
    /// Not evicted, and reported: its `emptyDir` data would be lost.
    EmptyDir,
}

pub(super) fn plan(pod: &Pod) -> PodPlan {
    let mirror = pod
        .metadata
        .annotations
        .as_ref()
        .is_some_and(|annotations| annotations.contains_key(MIRROR_ANNOTATION));
    if mirror {
        return PodPlan::Skip;
    }
    let controller = pod
        .metadata
        .owner_references
        .iter()
        .flatten()
        .find(|owner| owner.controller == Some(true));
    let empty_dir = pod
        .spec
        .as_ref()
        .and_then(|spec| spec.volumes.as_ref())
        .is_some_and(|volumes| volumes.iter().any(|volume| volume.empty_dir.is_some()));
    match controller {
        Some(owner) if owner.kind == "DaemonSet" => PodPlan::Skip,
        None => PodPlan::Unmanaged,
        Some(_) if empty_dir => PodPlan::EmptyDir,
        Some(_) => PodPlan::Evict,
    }
}

/// Drains the node `target`: its outcome (the cordon's), then one failed
/// outcome per pod that wasn't evicted. A failed eviction - a
/// PodDisruptionBudget saying no - fails that pod alone: the rest are still
/// evicted and the node stays cordoned. Each eviction gets `timeout`.
pub(super) async fn drain(
    client: kube::Client,
    target: &BulkTarget,
    timeout: Duration,
) -> Vec<ObjectOutcome> {
    let node = target.name.as_str();
    let cordoned = set_unschedulable(client.clone(), &target.kind, node, true).await;
    if let Err(failure) = cordoned {
        return vec![ObjectOutcome::of(target, Err(failure))];
    }
    let pods = Api::<Pod>::all(client.clone())
        .list(&ListParams::default().fields(&format!("spec.nodeName={node}")))
        .await;
    let pods = match pods {
        Ok(pods) => pods.items,
        Err(error) => {
            // The cordon went through: say so, since the node now takes no
            // new pods.
            let listed = ActionFailure::from_kube(&error);
            let failure = ActionFailure {
                message: format!(
                    "Cordoned, but its pods couldn't be listed: {}",
                    listed.message
                ),
                detail: listed.detail,
            };
            return vec![ObjectOutcome::of(target, Err(failure))];
        }
    };

    let mut failed = Vec::new();
    for pod in &pods {
        let namespace = pod.metadata.namespace.clone().unwrap_or_default();
        let name = pod.metadata.name.clone().unwrap_or_default();
        let part = format!("evict {namespace}/{name}");
        let uid = pod.metadata.uid.clone();
        let result = match plan(pod) {
            PodPlan::Skip => continue,
            PodPlan::Unmanaged => Err(not_evicted(UNMANAGED)),
            PodPlan::EmptyDir => Err(not_evicted(EMPTY_DIR)),
            PodPlan::Evict => evict(client.clone(), &namespace, &name, uid, timeout).await,
        };
        if let Err(failure) = result {
            failed.push(ObjectOutcome {
                target: target.clone(),
                part: Some(part),
                result: Err(failure),
            });
        }
    }
    let mut outcomes = vec![ObjectOutcome::of(target, Ok(()))];
    outcomes.extend(failed);
    outcomes
}

/// The `policy/v1` Eviction for `namespace/name`, only of the pod with `uid`.
///
/// Built here rather than by `Api::evict`: kube-core 4.2 sends its delete
/// options as `delete_options`, which the API server doesn't read - it takes
/// `deleteOptions` - so the UID precondition would be dropped.
pub(super) fn eviction(namespace: &str, name: &str, uid: Option<&str>) -> Value {
    let mut eviction = json!({ "apiVersion": "policy/v1", "kind": "Eviction",
        "metadata": { "name": name, "namespace": namespace } });
    if let Some(uid) = uid {
        eviction["deleteOptions"] = json!({ "preconditions": { "uid": uid } });
    }
    eviction
}

/// Evicts the pod `name` - only if it is still the one with `uid` - giving up
/// after `timeout`. Already gone is success.
async fn evict(
    client: kube::Client,
    namespace: &str,
    name: &str,
    uid: Option<String>,
    timeout: Duration,
) -> Result<(), ActionFailure> {
    let body = eviction(namespace, name, uid.as_deref())
        .to_string()
        .into_bytes();
    let request = http::Request::post(format!(
        "/api/v1/namespaces/{namespace}/pods/{name}/eviction"
    ))
    .header(http::header::CONTENT_TYPE, "application/json")
    .body(body);
    let Ok(request) = request else {
        return Err(not_evicted(
            "Not evicted: the eviction request couldn't be built.",
        ));
    };
    match tokio::time::timeout(timeout, client.request_text(request)).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(kube::Error::Api(status))) if status.code == 404 => Ok(()),
        Ok(Err(error)) => Err(ActionFailure::from_kube(&error)),
        Err(_) => Err(still_refused(timeout)),
    }
}

/// What a timed-out eviction reports: the client was still retrying a refusal.
fn still_refused(timeout: Duration) -> ActionFailure {
    let message = format!(
        "Not evicted within {} s: the API server kept refusing it, usually because a \
         PodDisruptionBudget allows no disruption right now.",
        timeout.as_secs()
    );
    ActionFailure {
        detail: message.clone(),
        message,
    }
}

/// Why a pod no controller manages isn't evicted.
const UNMANAGED: &str = "Not evicted: no controller manages this pod, so nothing would recreate \
                         it. Delete it yourself if it can go.";
/// Why a pod with an `emptyDir` volume isn't evicted.
const EMPTY_DIR: &str = "Not evicted: uses emptyDir (data would be lost).";

fn not_evicted(why: &str) -> ActionFailure {
    ActionFailure {
        message: why.to_string(),
        detail: why.to_string(),
    }
}
