//! The Resource panel's fixed taxonomy: which of the seven sections a
//! discovered kind belongs to.
//!
//! `resource-panel-grouping` design.md: the mapping is keyed on `(API group,
//! resource plural)`, never on the group alone - the core group (`""`) holds
//! kinds from every category (`Pod` is Workloads, `Service` is Network,
//! `ConfigMap` is Config, ...), so a group-only map cannot represent it. This
//! module owns the table and the lookup; it does not decide what a cluster
//! actually has - that is [`super::section::group_kinds`]'s job.

/// One of the Resource panel's sections, in the fixed display order design.md
/// specifies - declaration order here *is* that order, since [`Self::ALL`]
/// reads off it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Category {
    Workloads,
    Config,
    Network,
    Storage,
    Cluster,
    AccessControl,
    /// The fallback for anything the table below does not name - every CRD,
    /// and any built-in kind this table has not caught up with yet. Always
    /// last, never empty-but-hidden: a cluster with CRDs always fills it.
    CustomResources,
}

impl Category {
    /// Every section, in fixed display order - what
    /// [`super::section::group_kinds`] iterates to build sections in that
    /// order rather than alphabetically.
    pub(super) const ALL: [Category; 7] = [
        Category::Workloads,
        Category::Config,
        Category::Network,
        Category::Storage,
        Category::Cluster,
        Category::AccessControl,
        Category::CustomResources,
    ];

    /// The section's display name.
    pub(super) fn title(self) -> &'static str {
        match self {
            Category::Workloads => "Workloads",
            Category::Config => "Config",
            Category::Network => "Network",
            Category::Storage => "Storage",
            Category::Cluster => "Cluster",
            Category::AccessControl => "Access Control",
            Category::CustomResources => "Custom Resources",
        }
    }

    /// `group`/`plural`'s section, or [`Category::CustomResources`] when
    /// neither this table nor any earlier check names it - the fallback that
    /// keeps every discovered kind landing somewhere (design.md's "Grouping
    /// loses no kind").
    pub(super) fn for_gvk(group: &str, plural: &str) -> Category {
        TABLE
            .iter()
            .find(|(table_group, table_plural, _)| *table_group == group && *table_plural == plural)
            .map_or(Category::CustomResources, |(_, _, category)| *category)
    }
}

/// Whether `group`/`plural` is a kind the taxonomy names - a built-in API
/// kind - rather than one that falls through to [`Category::CustomResources`].
/// A list tab titles a kind outside it by its plural alone, its group in the
/// tooltip (`standard-resource-panels` 5.3).
pub(crate) fn is_built_in(group: &str, plural: &str) -> bool {
    Category::for_gvk(group, plural) != Category::CustomResources
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.title())
    }
}

/// `(API group, resource plural, section)`, transcribed from the Kubernetes
/// API reference's own category taxonomy (design.md's "The taxonomy") -
/// core-group entries (empty group) interleaved among every section rather
/// than grouped together, since that is how the reference itself has to
/// present them.
///
/// `PriorityClass` is filed under Cluster, not Config: design.md's own table
/// names Cluster for the core-group entries and Config for the full taxonomy,
/// which disagree with each other. Cluster wins here - it is the table that
/// enumerates every core-group kind's section, which is the one fact this
/// lookup exists to hold; the taxonomy list is a display-order outline that
/// happens to repeat it.
const TABLE: &[(&str, &str, Category)] = &[
    // Workloads
    ("", "pods", Category::Workloads),
    ("", "replicationcontrollers", Category::Workloads),
    ("apps", "deployments", Category::Workloads),
    ("apps", "replicasets", Category::Workloads),
    ("apps", "daemonsets", Category::Workloads),
    ("apps", "statefulsets", Category::Workloads),
    ("batch", "jobs", Category::Workloads),
    ("batch", "cronjobs", Category::Workloads),
    (
        "autoscaling",
        "horizontalpodautoscalers",
        Category::Workloads,
    ),
    ("policy", "poddisruptionbudgets", Category::Workloads),
    // Config
    ("", "configmaps", Category::Config),
    ("", "secrets", Category::Config),
    ("", "limitranges", Category::Config),
    ("", "resourcequotas", Category::Config),
    // Network
    ("", "services", Category::Network),
    ("", "endpoints", Category::Network),
    ("discovery.k8s.io", "endpointslices", Category::Network),
    ("networking.k8s.io", "ingresses", Category::Network),
    ("networking.k8s.io", "ingressclasses", Category::Network),
    ("networking.k8s.io", "networkpolicies", Category::Network),
    ("gateway.networking.k8s.io", "gateways", Category::Network),
    ("gateway.networking.k8s.io", "httproutes", Category::Network),
    // Storage
    ("", "persistentvolumes", Category::Storage),
    ("", "persistentvolumeclaims", Category::Storage),
    ("storage.k8s.io", "storageclasses", Category::Storage),
    ("storage.k8s.io", "csidrivers", Category::Storage),
    ("storage.k8s.io", "csinodes", Category::Storage),
    ("storage.k8s.io", "volumeattachments", Category::Storage),
    // Cluster
    ("", "namespaces", Category::Cluster),
    ("", "nodes", Category::Cluster),
    ("", "events", Category::Cluster),
    ("", "componentstatuses", Category::Cluster),
    ("scheduling.k8s.io", "priorityclasses", Category::Cluster),
    ("node.k8s.io", "runtimeclasses", Category::Cluster),
    ("coordination.k8s.io", "leases", Category::Cluster),
    // Access Control
    ("", "serviceaccounts", Category::AccessControl),
    (
        "rbac.authorization.k8s.io",
        "roles",
        Category::AccessControl,
    ),
    (
        "rbac.authorization.k8s.io",
        "rolebindings",
        Category::AccessControl,
    ),
    (
        "rbac.authorization.k8s.io",
        "clusterroles",
        Category::AccessControl,
    ),
    (
        "rbac.authorization.k8s.io",
        "clusterrolebindings",
        Category::AccessControl,
    ),
];
