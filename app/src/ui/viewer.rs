//! Which panel shows a referenced object - the one place that decides whether
//! a reference in a detail view is a link.
//!
//! `resource-links` section 2.1. Every detail view renders its references
//! through `ui::link`, which asks [`viewer_for`]; a kind gains a viewer by
//! being resolved here, with no change at any site that shows a reference to
//! it.

use crate::k8s::object_ref::ObjectRef;
use crate::ui::nav::NavTarget;

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
/// - a core `Namespace` opens the Pods list scoped to that namespace.
pub fn viewer_for(target: &ObjectRef) -> Option<Destination> {
    if !target.group.is_empty() {
        return None;
    }
    match target.kind.as_str() {
        "Pod" => {
            let namespace = target.namespace.as_ref()?;
            Some(Destination::unscoped(NavTarget::pod(
                namespace.clone(),
                target.name.clone(),
            )))
        }
        "Namespace" => Some(Destination {
            target: NavTarget::pods(),
            namespaces: vec![target.name.clone()],
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Destination, viewer_for};
    use crate::k8s::object_ref::ObjectRef;
    use crate::ui::nav::NavTarget;

    #[test]
    fn a_pod_reference_opens_that_pods_detail() {
        assert_eq!(
            viewer_for(&ObjectRef::core("Pod", "staging", "web-1")),
            Some(Destination {
                target: NavTarget::pod("staging", "web-1"),
                namespaces: Vec::new(),
            })
        );
    }

    #[test]
    fn a_namespace_reference_opens_the_pods_list_scoped_to_it() {
        assert_eq!(
            viewer_for(&ObjectRef::cluster_scoped("", "Namespace", "staging")),
            Some(Destination {
                target: NavTarget::pods(),
                namespaces: vec!["staging".into()],
            })
        );
    }

    #[test]
    fn a_kind_with_no_viewer_is_not_followable() {
        assert_eq!(
            viewer_for(&ObjectRef::namespaced(
                "apps",
                "ReplicaSet",
                "staging",
                "web"
            )),
            None
        );
        assert_eq!(
            viewer_for(&ObjectRef::core("ConfigMap", "staging", "app-config")),
            None
        );
    }

    /// A CRD's own `Pod` kind is not the built-in one, and a Pod with no
    /// namespace can't name one pod.
    #[test]
    fn only_the_core_pod_kind_resolves_and_only_with_a_namespace() {
        assert_eq!(
            viewer_for(&ObjectRef::namespaced("example.com", "Pod", "ns", "x")),
            None
        );
        assert_eq!(viewer_for(&ObjectRef::cluster_scoped("", "Pod", "x")), None);
    }
}
