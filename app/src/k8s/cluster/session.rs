use super::connection::{ClusterConnection, ConnectionState};
use super::context_health::ContextHealth;
use super::health::{self, HealthTransition};
use super::watch_registry::{PauseReason, WatchRegistry};
use crate::k8s::resource::pods::{PodsTable, watch_all_namespaces};
use gpui_kit::{App, AppContext as _, Entity, Global, WindowId};
use kube::Client;
use std::collections::{HashMap, HashSet};

mod holds;
mod pause_resume;
mod pods_watch;
mod registry;
#[cfg(test)]
mod test_support;

pub use registry::ClusterRegistry;

/// What a session's [`WatchRegistry`] counts subscriptions by: one entry per
/// watched kind, so two panels of one kind on one context share a stream and
/// the stream stops with the last of them (`standard-resource-panels` D3). The
/// context itself is the session's own key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WatchKey {
    /// The typed all-namespaces Pods watch.
    Pods,
    // UNWIRED: the generic list watch (`standard-resource-panels` 1.2) is the
    // first to subscribe with it; until then only tests construct it.
    #[allow(dead_code)]
    /// One discovered kind's generic `DynamicObject` watch. The kind's group,
    /// version, kind and plural pin its `ApiResource`.
    Kind(crate::k8s::cluster::discovery::DiscoveredKind),
}
