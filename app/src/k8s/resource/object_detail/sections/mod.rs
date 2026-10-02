//! The sections a kind gets after Overview, projected from the object's typed
//! `k8s-openapi` form. A kind not listed here - or an object that doesn't
//! deserialize as its kind - gets Overview only.
//!
//! This dispatch is where a kind gains a structured viewer; nothing that shows
//! a reference to it needs to change.

mod cluster;
mod common;
mod config;
mod network;
mod rbac;
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
        ("", "Node") => typed(object).map(|node| cluster::node(&node)),
        ("", "Namespace") => typed(object).map(|namespace| cluster::namespace(&namespace)),
        ("", "ConfigMap") => typed(object).map(|config_map| config::config_map(&config_map)),
        // Read from the (already redacted) JSON rather than the typed form:
        // the placeholders aren't valid base64, which `Secret::data` expects.
        ("", "Secret") => Some(config::secret(object)),
        ("", "PersistentVolumeClaim") => typed(object).map(|claim| storage::claim(&claim)),
        ("", "PersistentVolume") => typed(object).map(|volume| storage::volume(&volume)),
        ("storage.k8s.io", "StorageClass") => typed(object).map(|class| storage::class(&class)),
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
        ("batch", "CronJob") => {
            typed(object).map(|cron_job| workloads::cron_job(&cron_job, namespace))
        }
        ("", "Service") => typed(object).map(|service| network::service(&service)),
        ("", "Endpoints") => {
            typed(object).map(|endpoints| network::endpoints(&endpoints, namespace))
        }
        ("networking.k8s.io", "Ingress") => {
            typed(object).map(|ingress| network::ingress(&ingress, namespace))
        }
        ("networking.k8s.io", "NetworkPolicy") => {
            typed(object).map(|policy| network::network_policy(&policy))
        }
        ("discovery.k8s.io", "EndpointSlice") => {
            typed(object).map(|slice| network::endpoint_slice(&slice, namespace))
        }
        ("rbac.authorization.k8s.io", "Role") => typed(object).map(|role| rbac::role(&role)),
        ("rbac.authorization.k8s.io", "ClusterRole") => {
            typed(object).map(|role| rbac::cluster_role(&role))
        }
        ("rbac.authorization.k8s.io", "RoleBinding") => {
            typed(object).map(|binding| rbac::role_binding(&binding, namespace))
        }
        ("rbac.authorization.k8s.io", "ClusterRoleBinding") => {
            typed(object).map(|binding| rbac::cluster_role_binding(&binding))
        }
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
