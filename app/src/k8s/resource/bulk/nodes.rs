//! Cordon, Uncordon and Drain, as `kubectl` does them:
//!
//! - [`set_unschedulable`]: a Node's `spec.unschedulable` set, or cleared.
//! - [`drain`]: cordon, then evict each of the node's pods through the
//!   eviction subresource, which is what makes a PodDisruptionBudget hold. Like
//!   `kubectl drain` it leaves DaemonSet pods (their controller would put them
//!   straight back) and mirror pods (the kubelet owns them) alone. A pod that no
//!   controller manages is not evicted either: nothing would recreate it, which
//!   is why `kubectl drain` refuses such a node without `--force`. It is
//!   reported as not evicted rather than silently left running.

use super::{BulkTarget, ObjectOutcome};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::resource_actions::{ActionFailure, api_for, patch_params};
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::api::{EvictParams, ListParams, Patch};
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
    match controller {
        Some(owner) if owner.kind == "DaemonSet" => PodPlan::Skip,
        Some(_) => PodPlan::Evict,
        None => PodPlan::Unmanaged,
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
            return vec![ObjectOutcome::of(
                target,
                Err(ActionFailure::from_kube(&error)),
            )];
        }
    };

    let mut failed = Vec::new();
    for pod in &pods {
        let namespace = pod.metadata.namespace.clone().unwrap_or_default();
        let name = pod.metadata.name.clone().unwrap_or_default();
        let part = format!("evict {namespace}/{name}");
        let result = match plan(pod) {
            PodPlan::Skip => continue,
            PodPlan::Unmanaged => Err(unmanaged()),
            PodPlan::Evict => evict(client.clone(), &namespace, &name, timeout).await,
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

/// Evicts one pod, giving up after `timeout`. Already gone is success.
async fn evict(
    client: kube::Client,
    namespace: &str,
    name: &str,
    timeout: Duration,
) -> Result<(), ActionFailure> {
    let api = Api::<Pod>::namespaced(client, namespace);
    let params = EvictParams::default();
    match tokio::time::timeout(timeout, api.evict(name, &params)).await {
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

fn unmanaged() -> ActionFailure {
    let message = "Not evicted: no controller manages this pod, so nothing would recreate it. \
                   Delete it yourself if it can go."
        .to_string();
    ActionFailure {
        detail: message.clone(),
        message,
    }
}
