//! The bulk actions' rules and requests: which kinds each is offered for and how
//! a checked set is named ([`eligibility`]); against a server that records what
//! it is sent, the request each mutation makes per object and how a batch
//! reports an object that failed beside the ones that didn't ([`requests`]),
//! and a drain's evictions ([`drain`]). The shared kinds and server live here.

use super::{BulkTarget, Mutation, run};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::test_recorder::{Recorded, Recorder};
use kube::core::GroupVersionKind;
use serde_json::{Value, json};

pub(super) fn kind(
    group: &str,
    version: &str,
    kind: &str,
    plural: &str,
    namespaced: bool,
) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk(group, version, kind),
        plural: plural.to_string(),
        namespaced,
        verbs: Default::default(),
    }
}

pub(super) fn deployment() -> DiscoveredKind {
    kind("apps", "v1", "Deployment", "deployments", true)
}
pub(super) fn daemon_set() -> DiscoveredKind {
    kind("apps", "v1", "DaemonSet", "daemonsets", true)
}
pub(super) fn stateful_set() -> DiscoveredKind {
    kind("apps", "v1", "StatefulSet", "statefulsets", true)
}
pub(super) fn replica_set() -> DiscoveredKind {
    kind("apps", "v1", "ReplicaSet", "replicasets", true)
}
pub(super) fn node() -> DiscoveredKind {
    kind("", "v1", "Node", "nodes", false)
}
pub(super) fn config_map() -> DiscoveredKind {
    kind("", "v1", "ConfigMap", "configmaps", true)
}
pub(super) fn service() -> DiscoveredKind {
    kind("", "v1", "Service", "services", true)
}

pub(super) fn target(kind: DiscoveredKind, name: &str) -> BulkTarget {
    let namespace = kind.namespaced.then(|| "shop".to_string());
    BulkTarget {
        kind,
        namespace,
        name: name.to_string(),
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap()
}

/// An object of `kind` named `name`, as the API answers a patch.
pub(super) fn object(kind: &DiscoveredKind, name: &str) -> Value {
    let api_version = if kind.gvk.group.is_empty() {
        kind.gvk.version.clone()
    } else {
        format!("{}/{}", kind.gvk.group, kind.gvk.version)
    };
    json!({ "apiVersion": api_version, "kind": kind.gvk.kind, "metadata": { "name": name } })
}

pub(super) fn success() -> Value {
    json!({ "kind": "Status", "apiVersion": "v1", "status": "Success", "code": 200 })
}

pub(super) fn refusal(code: u16, reason: &str, message: &str) -> Value {
    json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure",
        "reason": reason, "code": code, "message": message })
}

/// Runs `mutation` on `targets` against a server answering with `answer`, and
/// returns the requests it got and the outcomes.
pub(super) fn record(
    mutation: Mutation,
    targets: &[BulkTarget],
    answer: impl Fn(&Recorded) -> (&'static str, Value) + Send + Sync + 'static,
) -> (Vec<Recorded>, Vec<super::ObjectOutcome>) {
    let runtime = runtime();
    let (server, client) = Recorder::start_answering(runtime.handle(), answer);
    let outcomes = runtime.block_on(run(client, &mutation, targets));
    (server.requests(), outcomes)
}

pub(super) fn sorted_targets(requests: &[Recorded]) -> Vec<String> {
    let mut targets: Vec<String> = requests
        .iter()
        .map(|request| format!("{} {}", request.method, path(&request.target)))
        .collect();
    targets.sort();
    targets
}

pub(super) fn path(target: &str) -> &str {
    target.split('?').next().unwrap_or(target)
}

mod drain;
mod eligibility;
mod requests;
