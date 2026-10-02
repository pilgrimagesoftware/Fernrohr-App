//! A typed reference from one Kubernetes object to another, as a detail view
//! finds it in the object it shows: an owner reference, a node name, a volume's
//! ConfigMap, an env source's Secret.
//!
//! Built once, where the reference is read out of the API object, so nothing
//! downstream re-derives a kind or namespace from display text. Whether a
//! reference can be *followed* is not this type's business - that is
//! `ui::nav::viewer_for`'s, which is the one place that knows which kinds have
//! a panel.

use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;

/// One referenced object: which kind, and which object of it.
///
/// Identity for "which viewer" is `(group, kind)` with no version: owner
/// references carry one, but a viewer is per kind, and matching on it would let
/// an `apps/v1` vs `apps/v1beta2` difference stop a reference from resolving.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ObjectRef {
    /// The API group, empty for the core group.
    pub group: String,
    pub kind: String,
    /// The object's namespace, or `None` for a cluster-scoped kind. Filled in
    /// at construction: a bare name read out of a namespaced object resolves in
    /// that object's namespace, so the render side never reasons about scope.
    pub namespace: Option<String>,
    pub name: String,
}

impl ObjectRef {
    /// A namespaced object named by bare name from inside `namespace` - how a
    /// pod names its ConfigMaps, Secrets, claims and service account.
    pub fn namespaced(
        group: impl Into<String>,
        kind: impl Into<String>,
        namespace: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            group: group.into(),
            kind: kind.into(),
            namespace: Some(namespace.into()),
            name: name.into(),
        }
    }

    /// A cluster-scoped object - how a pod names its node.
    pub fn cluster_scoped(
        group: impl Into<String>,
        kind: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            group: group.into(),
            kind: kind.into(),
            namespace: None,
            name: name.into(),
        }
    }

    /// A core-group (`v1`) namespaced object, the common case in a pod spec.
    pub fn core(kind: &str, namespace: &str, name: &str) -> Self {
        Self::namespaced("", kind, namespace, name)
    }

    /// An owner, as `metadata.ownerReferences` records it. Owners live in
    /// their dependent's namespace (the API forbids cross-namespace owners), so
    /// `namespace` is the dependent's own; `None` for a cluster-scoped
    /// dependent. A cluster-scoped owner of a namespaced dependent is corrected
    /// by `viewer_for`, which knows the owner kind's real scope from discovery.
    pub fn from_owner(owner: &OwnerReference, namespace: Option<&str>) -> Self {
        Self {
            group: group_of(&owner.api_version).to_string(),
            kind: owner.kind.clone(),
            namespace: namespace.map(str::to_string),
            name: owner.name.clone(),
        }
    }

    /// `Kind/name`, the kubectl-style form a list of mixed kinds reads as.
    pub fn qualified_name(&self) -> String {
        format!("{}/{}", self.kind, self.name)
    }
}

/// The group half of an `apiVersion`: `apps/v1` is `apps`, and the core
/// group's bare `v1` is the empty group.
fn group_of(api_version: &str) -> &str {
    api_version
        .rsplit_once('/')
        .map(|(group, _version)| group)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::ObjectRef;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;

    fn owner(api_version: &str, kind: &str, name: &str) -> OwnerReference {
        OwnerReference {
            api_version: api_version.into(),
            kind: kind.into(),
            name: name.into(),
            uid: "uid".into(),
            ..Default::default()
        }
    }

    #[test]
    fn an_owners_api_version_splits_into_its_group() {
        let replica_set = ObjectRef::from_owner(&owner("apps/v1", "ReplicaSet", "web"), Some("ns"));
        assert_eq!(replica_set.group, "apps");
        assert_eq!(replica_set.kind, "ReplicaSet");

        let core = ObjectRef::from_owner(&owner("v1", "Node", "node-a"), Some("ns"));
        assert_eq!(core.group, "", "core `v1` is the empty group");

        let crd =
            ObjectRef::from_owner(&owner("argoproj.io/v1alpha1", "Rollout", "web"), Some("ns"));
        assert_eq!(crd.group, "argoproj.io");
    }

    #[test]
    fn an_owner_inherits_its_dependents_namespace() {
        let namespaced = ObjectRef::from_owner(&owner("apps/v1", "ReplicaSet", "web"), Some("ns"));
        assert_eq!(namespaced.namespace.as_deref(), Some("ns"));

        let of_cluster_scoped = ObjectRef::from_owner(&owner("v1", "Node", "node-a"), None);
        assert_eq!(of_cluster_scoped.namespace, None);
    }

    #[test]
    fn cluster_scoped_references_have_no_namespace() {
        let node = ObjectRef::cluster_scoped("", "Node", "node-a");
        assert_eq!(node.namespace, None);
        assert!(node.group.is_empty());

        let config_map = ObjectRef::core("ConfigMap", "staging", "app-config");
        assert_eq!(config_map.namespace.as_deref(), Some("staging"));
        assert_eq!(config_map.qualified_name(), "ConfigMap/app-config");
    }

    #[test]
    fn identity_ignores_the_owners_version() {
        let v1 = ObjectRef::from_owner(&owner("apps/v1", "ReplicaSet", "web"), Some("ns"));
        let beta = ObjectRef::from_owner(&owner("apps/v1beta2", "ReplicaSet", "web"), Some("ns"));
        assert_eq!(v1, beta);
    }
}
