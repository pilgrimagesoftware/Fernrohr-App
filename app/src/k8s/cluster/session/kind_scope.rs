//! Which shared kind watch serves a subscriber that only needs one namespace (#446).
//!
//! A panel that shows one object (a detail panel, the Controls field's child watches)
//! needs one namespace of a kind, not the whole cluster. [`scope_for`] picks the watch:
//! an open cluster-wide watch that works serves it, so nothing new starts; otherwise a
//! watch limited to that namespace does. That second case is also the fallback for a user
//! whose RBAC refuses cluster-wide lists: the refused cluster-wide watch is never shared,
//! and a namespaced list is what such a user is allowed.

use super::KindKey;
use crate::k8s::resource::object_list::ObjectsTable;
use gpui_kit::Entity;

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
