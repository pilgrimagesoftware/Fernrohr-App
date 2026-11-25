//! Which shared kind watch serves a subscriber that only needs one namespace (#446).
//!
//! A panel that shows one object (a detail panel, the Controls field's child watches)
//! needs one namespace of a kind, not the whole cluster. [`scope_for`] picks the watch:
//! an open cluster-wide watch that works serves it, so nothing new starts; otherwise a
//! watch limited to that namespace does. That second case is also the fallback for a user
//! whose RBAC refuses cluster-wide lists: the refused cluster-wide watch is never shared,
//! and a namespaced list is what such a user is allowed.

use super::{ClusterRegistry, KindKey};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::object_list::ObjectsTable;
use gpui_kit::{App, Entity};

/// What the context's cluster-wide watch of a kind looks like to a new subscriber.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClusterWide {
    /// No panel holds one.
    Absent,
    /// Held, and the server hasn't refused it.
    Serving,
    /// Held, but the server refused the cluster-wide list (a 403).
    Refused,
}

/// The namespace to watch for a subscriber that needs `namespace` of a kind: `None` for the
/// cluster-wide watch, `Some` for one limited to the namespace.
///
/// A cluster-scoped kind (`kind_namespaced` false) or a subscriber with no namespace always
/// uses the cluster-wide watch.
pub(crate) fn scope_for(
    kind_namespaced: bool,
    namespace: Option<&str>,
    cluster_wide: ClusterWide,
) -> Option<String> {
    let namespace = namespace.filter(|_| kind_namespaced)?;
    match cluster_wide {
        ClusterWide::Serving => None,
        ClusterWide::Absent | ClusterWide::Refused => Some(namespace.to_string()),
    }
}

/// A subscription to a shared kind watch: the table to read, and the key to release it by.
/// The table may hold more than the subscriber's namespace when it is the cluster-wide one,
/// so a reader still matches by namespace or uid.
#[derive(Clone)]
pub(crate) struct KindSubscription {
    pub(crate) key: KindKey,
    pub(crate) table: Entity<ObjectsTable>,
}

/// A subscriber's hold on the watch serving one namespace of a kind, which moves itself off
/// a cluster-wide watch the server comes to refuse (#457).
///
/// [`scope_for`] only shares a cluster-wide watch that serves, but one that served when the
/// subscriber attached can be refused later - a restart after a role change, say. Without
/// moving, the subscriber would show "can't list" although a watch of its namespace works.
/// The holder calls [`Self::reroute_if_refused`] whenever its table changes.
pub(crate) struct NamespaceFollow {
    context_name: String,
    kind: DiscoveredKind,
    namespace: Option<String>,
    subscription: KindSubscription,
}

impl NamespaceFollow {
    /// Subscribes to the watch that serves `namespace` of `kind` on `context_name`.
    pub(crate) fn subscribe(
        cx: &mut App,
        context_name: &str,
        client: kube::Client,
        kind: &DiscoveredKind,
        namespace: Option<&str>,
    ) -> Self {
        let subscription =
            ClusterRegistry::subscribe_kind_in(cx, context_name, client, kind, namespace);
        Self {
            context_name: context_name.to_string(),
            kind: kind.clone(),
            namespace: namespace.map(str::to_string),
            subscription,
        }
    }

    /// A follow of `table` that belongs to no session, for a test that fills the table
    /// itself: it never moves, and releasing it releases nothing.
    #[cfg(test)]
    pub(crate) fn detached(kind: &DiscoveredKind, table: Entity<ObjectsTable>) -> Self {
        Self {
            context_name: String::new(),
            kind: kind.clone(),
            namespace: None,
            subscription: KindSubscription {
                key: KindKey::all(kind),
                table,
            },
        }
    }

    /// The table to read now. It changes when the follow moves, so a holder that observes
    /// it observes the new one after [`Self::reroute_if_refused`] says so.
    pub(crate) fn table(&self) -> &Entity<ObjectsTable> {
        &self.subscription.table
    }

    /// Moves to a watch of the subscriber's own namespace if the cluster-wide watch it
    /// shares has been refused, releasing that one. Returns whether it moved. Never moves
    /// a subscriber that needs every namespace, or a cluster-scoped kind's: nothing
    /// narrower would serve them.
    pub(crate) fn reroute_if_refused(&mut self, cx: &mut App) -> bool {
        let shares_cluster_wide = self.subscription.key.namespace.is_none();
        if !shares_cluster_wide
            || !self.kind.namespaced
            || self.namespace.is_none()
            || self.subscription.table.read(cx).refused().is_none()
        {
            return false;
        }
        let Some(client) = ClusterRegistry::session_client(cx, &self.context_name) else {
            return false;
        };
        let moved = ClusterRegistry::subscribe_kind_in(
            cx,
            &self.context_name,
            client,
            &self.kind,
            self.namespace.as_deref(),
        );
        let old = std::mem::replace(&mut self.subscription, moved);
        ClusterRegistry::unsubscribe_kind_in(cx, &self.context_name, &old.key);
        true
    }

    /// Releases the hold, as the holder closing does.
    pub(crate) fn release(&self, cx: &mut App) {
        ClusterRegistry::unsubscribe_kind_in(cx, &self.context_name, &self.subscription.key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_serving_cluster_wide_watch_serves_a_namespace() {
        assert_eq!(scope_for(true, Some("staging"), ClusterWide::Serving), None);
    }

    #[test]
    fn with_no_cluster_wide_watch_the_namespace_is_watched_alone() {
        assert_eq!(
            scope_for(true, Some("staging"), ClusterWide::Absent),
            Some("staging".to_string())
        );
    }

    #[test]
    fn a_refused_cluster_wide_watch_falls_back_to_the_namespace() {
        assert_eq!(
            scope_for(true, Some("staging"), ClusterWide::Refused),
            Some("staging".to_string())
        );
    }

    #[test]
    fn a_cluster_scoped_kind_is_always_cluster_wide() {
        for state in [
            ClusterWide::Absent,
            ClusterWide::Serving,
            ClusterWide::Refused,
        ] {
            assert_eq!(scope_for(false, Some("staging"), state), None);
        }
    }

    #[test]
    fn a_subscriber_with_no_namespace_is_cluster_wide() {
        for state in [
            ClusterWide::Absent,
            ClusterWide::Serving,
            ClusterWide::Refused,
        ] {
            assert_eq!(scope_for(true, None, state), None);
        }
    }
}
