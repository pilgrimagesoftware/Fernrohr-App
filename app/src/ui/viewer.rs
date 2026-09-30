//! Which panel shows a referenced object - the one place that decides whether
//! a reference in a detail view is a link.
//!
//! `resource-links` section 2.1. Every detail view renders its references
//! through `ui::link`, which asks [`viewer_for`]; a kind gains a viewer by
//! being resolved here, with no change at any site that shows a reference to
//! it.

use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::object_ref::ObjectRef;
use crate::ui::nav::{NavTarget, ObjectTarget};

/// Where following a reference lands: the panel target, and the namespace
/// scope to give it - the Pods list opened from a Namespace reference is scoped
/// to that namespace, everything else to none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Destination {
    pub target: NavTarget,
    pub namespaces: Vec<String>,
}

impl Destination {
    fn unscoped(target: NavTarget) -> Self {
        Self {
            target,
            namespaces: Vec::new(),
        }
    }
}

/// The panel `target` opens, or `None` when the application has no panel for
/// its kind - in which case the reference is plain text, never a link that
/// leads nowhere.
///
/// - a core `Pod` opens its detail panel;
/// - a core `Namespace` opens the Pods list scoped to that namespace;
/// - any other kind the context's discovery reports (`kinds`) opens the
///   generic object viewer - with the namespace dropped for a cluster-scoped
///   kind, and required for a namespaced one;
/// - anything else, including every other kind while discovery hasn't loaded
///   (`kinds` is `None`), is `None`.
pub fn viewer_for(target: &ObjectRef, kinds: Option<&[DiscoveredKind]>) -> Option<Destination> {
    if target.group.is_empty() {
        match target.kind.as_str() {
            "Pod" => {
                let namespace = target.namespace.as_ref()?;
                return Some(Destination::unscoped(NavTarget::pod(
                    namespace.clone(),
                    target.name.clone(),
                )));
            }
            "Namespace" => {
                return Some(Destination {
                    target: NavTarget::pods(),
                    namespaces: vec![target.name.clone()],
                });
            }
            _ => {}
        }
    }
    let kind = kinds?
        .iter()
        .find(|kind| kind.gvk.group == target.group && kind.gvk.kind == target.kind)?;
    let namespace = if kind.namespaced {
        Some(target.namespace.clone()?)
    } else {
        None
    };
    Some(Destination::unscoped(NavTarget::Object(ObjectTarget {
        kind: kind.clone(),
        namespace,
        name: target.name.clone(),
    })))
}

#[cfg(test)]
mod tests {
    use super::{Destination, viewer_for};
    use crate::k8s::cluster::discovery::DiscoveredKind;
    use crate::k8s::object_ref::ObjectRef;
    use crate::ui::nav::{NavTarget, ObjectTarget};
    use kube::core::GroupVersionKind;

    fn discovered(group: &str, kind: &str, namespaced: bool) -> DiscoveredKind {
        DiscoveredKind {
            gvk: GroupVersionKind::gvk(group, "v1", kind),
            plural: format!("{}s", kind.to_lowercase()),
            namespaced,
        }
    }

    fn cluster_kinds() -> Vec<DiscoveredKind> {
        vec![
            discovered("apps", "ReplicaSet", true),
            discovered("", "Node", false),
            discovered("", "ConfigMap", true),
        ]
    }

    /// 5.4: a discovered kind opens the generic viewer, over the discovered
    /// kind itself - version, plural and scope included.
    #[test]
    fn a_discovered_kind_opens_the_object_viewer() {
        let kinds = cluster_kinds();
        assert_eq!(
            viewer_for(
                &ObjectRef::namespaced("apps", "ReplicaSet", "staging", "web"),
                Some(&kinds)
            ),
            Some(Destination {
                target: NavTarget::Object(ObjectTarget {
                    kind: kinds[0].clone(),
                    namespace: Some("staging".into()),
                    name: "web".into(),
                }),
                namespaces: Vec::new(),
            })
        );
    }

    /// A cluster-scoped kind drops whatever namespace the reference inherited
    /// (an owner reference fills in its dependent's), since discovery knows
    /// the kind's real scope.
    #[test]
    fn a_cluster_scoped_kind_drops_an_inherited_namespace() {
        let kinds = cluster_kinds();
        let Some(Destination {
            target: NavTarget::Object(object),
            ..
        }) = viewer_for(&ObjectRef::core("Node", "staging", "node-a"), Some(&kinds))
        else {
            panic!("a discovered Node is viewable");
        };
        assert_eq!(object.namespace, None);
        assert_eq!(object.name, "node-a");
    }

    /// Before discovery loads, and for a kind the cluster doesn't report (a
    /// removed CRD), a reference is plain text.
    #[test]
    fn an_undiscovered_kind_is_not_followable() {
        let config_map = ObjectRef::core("ConfigMap", "staging", "app-config");
        assert_eq!(
            viewer_for(&config_map, None),
            None,
            "discovery not loaded yet"
        );
        assert_eq!(
            viewer_for(
                &ObjectRef::namespaced("argoproj.io", "Rollout", "staging", "web"),
                Some(&cluster_kinds())
            ),
            None,
            "not in this cluster's discovery"
        );
        assert!(viewer_for(&config_map, Some(&cluster_kinds())).is_some());
    }

    #[test]
    fn a_pod_reference_opens_that_pods_detail() {
        assert_eq!(
            viewer_for(&ObjectRef::core("Pod", "staging", "web-1"), None),
            Some(Destination {
                target: NavTarget::pod("staging", "web-1"),
                namespaces: Vec::new(),
            })
        );
    }

    #[test]
    fn a_namespace_reference_opens_the_pods_list_scoped_to_it() {
        assert_eq!(
            viewer_for(&ObjectRef::cluster_scoped("", "Namespace", "staging"), None),
            Some(Destination {
                target: NavTarget::pods(),
                namespaces: vec!["staging".into()],
            })
        );
    }

    /// With no discovery to consult, only Pod and Namespace resolve.
    #[test]
    fn a_kind_with_no_viewer_is_not_followable() {
        assert_eq!(
            viewer_for(
                &ObjectRef::namespaced("apps", "ReplicaSet", "staging", "web"),
                None
            ),
            None
        );
        assert_eq!(
            viewer_for(&ObjectRef::core("ConfigMap", "staging", "app-config"), None),
            None
        );
    }

    /// A CRD's own `Pod` kind is not the built-in one, and a Pod with no
    /// namespace can't name one pod.
    #[test]
    fn only_the_core_pod_kind_resolves_and_only_with_a_namespace() {
        assert_eq!(
            viewer_for(
                &ObjectRef::namespaced("example.com", "Pod", "ns", "x"),
                None
            ),
            None
        );
        assert_eq!(
            viewer_for(&ObjectRef::cluster_scoped("", "Pod", "x"), None),
            None
        );
    }
}
