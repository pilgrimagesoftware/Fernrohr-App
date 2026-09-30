//! The sections a kind gets after Overview, projected from the object's typed
//! `k8s-openapi` form. A kind not listed here - or an object that doesn't
//! deserialize as its kind - gets Overview only.
//!
//! This dispatch is where a kind gains a structured viewer; nothing that shows
//! a reference to it needs to change.

mod common;
mod config;
mod node;
mod service_account;
mod storage;
mod workloads;

use super::model::ObjectSection;
use crate::k8s::cluster::discovery::DiscoveredKind;
use kube::api::DynamicObject;
use serde::de::DeserializeOwned;

/// `object`'s kind-specific sections, in display order.
pub(super) fn sections_for(kind: &DiscoveredKind, object: &DynamicObject) -> Vec<ObjectSection> {
    let namespace = object.metadata.namespace.as_deref().unwrap_or_default();
    match (kind.gvk.group.as_str(), kind.gvk.kind.as_str()) {
        ("", "Node") => typed(object).map(|node| node::sections(&node)),
        ("", "ConfigMap") => typed(object).map(|config_map| config::config_map(&config_map)),
        // Read from the (already redacted) JSON rather than the typed form:
        // the placeholders aren't valid base64, which `Secret::data` expects.
        ("", "Secret") => Some(config::secret(object)),
        ("", "PersistentVolumeClaim") => typed(object).map(|claim| storage::claim(&claim)),
        ("", "ServiceAccount") => {
            typed(object).map(|account| service_account::sections(&account, namespace))
        }
        ("apps", "ReplicaSet") => typed(object).map(|set| workloads::replica_set(&set)),
        ("apps", "Deployment") => {
            typed(object).map(|deployment| workloads::deployment(&deployment))
        }
        ("apps", "StatefulSet") => {
            typed(object).map(|set| workloads::stateful_set(&set, namespace))
        }
        ("apps", "DaemonSet") => typed(object).map(|set| workloads::daemon_set(&set)),
        ("batch", "Job") => typed(object).map(|job| workloads::job(&job)),
        _ => None,
    }
    .unwrap_or_default()
}

/// `object` as its typed form, or `None` if it doesn't deserialize as one.
fn typed<T: DeserializeOwned>(object: &DynamicObject) -> Option<T> {
    serde_json::to_value(object)
        .ok()
        .and_then(|value| serde_json::from_value(value).ok())
}
