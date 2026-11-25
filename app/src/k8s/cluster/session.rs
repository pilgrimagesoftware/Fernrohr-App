use super::connection::{ClusterConnection, ConnectionState};
use super::context_health::ContextHealth;
use super::health::{self, HealthTransition};
use super::watch_registry::{PauseReason, WatchRegistry};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::events_browser::{EventsTable, watch_events};
use crate::k8s::resource::helm::{HelmReleasesTable, HelmScope, watch_helm_releases};
use crate::k8s::resource::object_list::{ObjectsTable, watch_kind};
use crate::k8s::resource::pods::{PodsTable, watch_pods};
use gpui_kit::{App, AppContext as _, Entity, Global, WindowId};
use kube::Client;
use std::collections::{HashMap, HashSet};

mod events_watch;
mod helm_watch;
mod holder;
mod holds;
mod kind_scope;
mod kind_watch;
mod live_agents;
mod pause_resume;
mod pods_scope;
mod pods_watch;
mod reconnect;
mod registry;
#[cfg(test)]
mod test_support;

pub use holder::{AgentId, Holder};
pub(crate) use kind_scope::{KindSubscription, NamespaceFollow};
pub use live_agents::LiveAgents;
pub(crate) use pods_scope::PodsFollow;
pub use registry::ClusterRegistry;

/// One shared kind watch: the kind, and the namespace it is limited to - `None` for a
/// cluster-wide watch (`Api::all`), which serves every namespace.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct KindKey {
    pub(crate) kind: DiscoveredKind,
    pub(crate) namespace: Option<String>,
}

impl KindKey {
    /// `kind` across every namespace.
    pub(crate) fn all(kind: &DiscoveredKind) -> Self {
        Self {
            kind: kind.clone(),
            namespace: None,
        }
    }
}

/// What a session's [`WatchRegistry`] counts subscriptions by: one entry per
/// watched kind, so two panels of one kind on one context share a stream and
/// the stream stops with the last of them (`standard-resource-panels` D3). The
/// context itself is the session's own key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WatchKey {
    /// The typed all-namespaces Pods watch.
    Pods,
    /// The typed Pods watch of one namespace (#457), for a reader that needs only that
    /// namespace's pods when the all-namespaces watch is refused or not open.
    PodsIn(String),
    /// One discovered kind's generic `DynamicObject` watch, cluster-wide or
    /// over one namespace (#446). The kind's group, version, kind and plural pin
    /// its `ApiResource`.
    Kind(KindKey),
    /// The typed all-namespaces Event watch behind the events browser
    /// (`events-browser` D1), shared by every Event consumer on the context.
    Events,
    /// The metadata-only Helm release watch over one scope (helm-releases
    /// D1), shared by every Helm panel of that scope on the context.
    HelmReleases(HelmScope),
}
