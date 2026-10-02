//! Which icon stands for a resource kind (`resource-icons` spec): the
//! Kubernetes community icon set's full-colour artwork for every built-in kind
//! it covers, and a fallback in the same style for everything else - so no
//! kind is ever drawn without one.
//!
//! The set's files live in `assets/icons/kubernetes/` (Apache-2.0, see the
//! `LICENSE` and `README.md` there); the three fallbacks in
//! `assets/icons/fallback/` are Fernrohr's own. This module decides which
//! icon; [`element`] draws one beside text.

mod element;
mod inline;
#[cfg(test)]
pub(crate) use element::test_hooks;
pub use element::{IconSize, kind_icon};
pub use inline::InlineKindIcon;
#[cfg(test)]
pub(crate) use inline::test_support;

/// One bundled icon. A closed set: every file this app ships is a variant, so
/// adding an icon is adding a variant here and a file, and the tests walk
/// [`KindIcon::ALL`] to check each one loads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KindIcon {
    Pod,
    Deployment,
    ReplicaSet,
    StatefulSet,
    DaemonSet,
    Job,
    CronJob,
    HorizontalPodAutoscaler,
    Service,
    Endpoints,
    Ingress,
    NetworkPolicy,
    ConfigMap,
    Secret,
    LimitRange,
    ResourceQuota,
    PersistentVolume,
    PersistentVolumeClaim,
    StorageClass,
    Namespace,
    Node,
    ServiceAccount,
    Role,
    RoleBinding,
    ClusterRole,
    ClusterRoleBinding,
    CustomResourceDefinition,
    PodSecurityPolicy,
    /// A container inside a pod - not a kind, but listed like one.
    Container,
    /// An instance of a custom resource: a kind in a group Kubernetes
    /// doesn't define.
    CustomResource,
    /// A built-in kind the set has no icon for.
    Kind,
}

impl KindIcon {
    /// Every icon, in declaration order, for the tests to walk.
    #[cfg(test)]
    pub const ALL: [KindIcon; 31] = [
        Self::Pod,
        Self::Deployment,
        Self::ReplicaSet,
        Self::StatefulSet,
        Self::DaemonSet,
        Self::Job,
        Self::CronJob,
        Self::HorizontalPodAutoscaler,
        Self::Service,
        Self::Endpoints,
        Self::Ingress,
        Self::NetworkPolicy,
        Self::ConfigMap,
        Self::Secret,
        Self::LimitRange,
        Self::ResourceQuota,
        Self::PersistentVolume,
        Self::PersistentVolumeClaim,
        Self::StorageClass,
        Self::Namespace,
        Self::Node,
        Self::ServiceAccount,
        Self::Role,
        Self::RoleBinding,
        Self::ClusterRole,
        Self::ClusterRoleBinding,
        Self::CustomResourceDefinition,
        Self::PodSecurityPolicy,
        Self::Container,
        Self::CustomResource,
        Self::Kind,
    ];

    /// The icon's SVG source, embedded in the binary.
    pub fn svg(self) -> &'static [u8] {
        macro_rules! set {
            ($file:literal) => {
                include_bytes!(concat!("../../assets/icons/kubernetes/", $file, ".svg"))
            };
        }
        macro_rules! fallback {
            ($file:literal) => {
                include_bytes!(concat!("../../assets/icons/fallback/", $file, ".svg"))
            };
        }
        match self {
            Self::Pod => set!("pod"),
            Self::Deployment => set!("deploy"),
            Self::ReplicaSet => set!("rs"),
            Self::StatefulSet => set!("sts"),
            Self::DaemonSet => set!("ds"),
            Self::Job => set!("job"),
            Self::CronJob => set!("cronjob"),
            Self::HorizontalPodAutoscaler => set!("hpa"),
            Self::Service => set!("svc"),
            Self::Endpoints => set!("ep"),
            Self::Ingress => set!("ing"),
            Self::NetworkPolicy => set!("netpol"),
            Self::ConfigMap => set!("cm"),
            Self::Secret => set!("secret"),
            Self::LimitRange => set!("limits"),
            Self::ResourceQuota => set!("quota"),
            Self::PersistentVolume => set!("pv"),
            Self::PersistentVolumeClaim => set!("pvc"),
            Self::StorageClass => set!("sc"),
            Self::Namespace => set!("ns"),
            Self::Node => set!("node"),
            Self::ServiceAccount => set!("sa"),
            Self::Role => set!("role"),
            Self::RoleBinding => set!("rb"),
            Self::ClusterRole => set!("c-role"),
            Self::ClusterRoleBinding => set!("crb"),
            Self::CustomResourceDefinition => set!("crd"),
            Self::PodSecurityPolicy => set!("psp"),
            Self::Container => fallback!("container"),
            Self::CustomResource => fallback!("custom-resource"),
            Self::Kind => fallback!("kind"),
        }
    }
}

/// The icon for `kind` in API group `group` (`""` for the core group).
///
/// Keyed by both, so a CRD that happens to be named `Pod` in its own group
/// gets the custom-resource icon, not Pod's. A built-in group's kind the set
/// doesn't cover gets the generic [`KindIcon::Kind`]; any other group's kind
/// is a custom resource.
pub fn for_kind(group: &str, kind: &str) -> KindIcon {
    match (group, kind) {
        ("", "Pod") => KindIcon::Pod,
        ("", "Service") => KindIcon::Service,
        ("", "Endpoints") => KindIcon::Endpoints,
        ("", "ConfigMap") => KindIcon::ConfigMap,
        ("", "Secret") => KindIcon::Secret,
        ("", "LimitRange") => KindIcon::LimitRange,
        ("", "ResourceQuota") => KindIcon::ResourceQuota,
        ("", "PersistentVolume") => KindIcon::PersistentVolume,
        ("", "PersistentVolumeClaim") => KindIcon::PersistentVolumeClaim,
        ("", "Namespace") => KindIcon::Namespace,
        ("", "Node") => KindIcon::Node,
        ("", "ServiceAccount") => KindIcon::ServiceAccount,
        ("apps", "Deployment") => KindIcon::Deployment,
        ("apps", "ReplicaSet") => KindIcon::ReplicaSet,
        ("apps", "StatefulSet") => KindIcon::StatefulSet,
        ("apps", "DaemonSet") => KindIcon::DaemonSet,
        ("batch", "Job") => KindIcon::Job,
        ("batch", "CronJob") => KindIcon::CronJob,
        ("autoscaling", "HorizontalPodAutoscaler") => KindIcon::HorizontalPodAutoscaler,
        ("networking.k8s.io", "Ingress") => KindIcon::Ingress,
        ("networking.k8s.io", "NetworkPolicy") => KindIcon::NetworkPolicy,
        ("storage.k8s.io", "StorageClass") => KindIcon::StorageClass,
        ("rbac.authorization.k8s.io", "Role") => KindIcon::Role,
        ("rbac.authorization.k8s.io", "RoleBinding") => KindIcon::RoleBinding,
        ("rbac.authorization.k8s.io", "ClusterRole") => KindIcon::ClusterRole,
        ("rbac.authorization.k8s.io", "ClusterRoleBinding") => KindIcon::ClusterRoleBinding,
        ("apiextensions.k8s.io", "CustomResourceDefinition") => KindIcon::CustomResourceDefinition,
        ("policy", "PodSecurityPolicy") => KindIcon::PodSecurityPolicy,
        _ => fallback_for(group),
    }
}

/// The icon for what a panel shows: its kind's, or Pod's for one pod's
/// detail, or a container's for the Logs panel, which streams one container.
pub fn for_target(target: &crate::ui::nav::NavTarget) -> KindIcon {
    use crate::ui::nav::NavTarget;
    match target {
        NavTarget::Kind(kind) => for_kind(&kind.gvk.group, &kind.gvk.kind),
        NavTarget::Object(object) => for_kind(&object.kind.gvk.group, &object.kind.gvk.kind),
        NavTarget::Pod(_) => KindIcon::Pod,
        NavTarget::Logs => KindIcon::Container,
    }
}

// UNWIRED: the container cards draw it in `resource-kind-icons` 3.3.
#[allow(dead_code)]
/// The icon for a container listed in pod detail.
pub fn for_container() -> KindIcon {
    KindIcon::Container
}

/// A kind the set has no icon for: generic in a group Kubernetes defines, a
/// custom resource in any other.
fn fallback_for(group: &str) -> KindIcon {
    if is_builtin_group(group) {
        KindIcon::Kind
    } else {
        KindIcon::CustomResource
    }
}

/// The API groups the Kubernetes API server itself serves. Anything else is
/// a CRD's - including CRD groups under `k8s.io` such as the Gateway API's
/// `gateway.networking.k8s.io` or `snapshot.storage.k8s.io`, which a suffix
/// rule would wrongly call built-in.
const BUILTIN_GROUPS: &[&str] = &[
    "",
    "admissionregistration.k8s.io",
    "apiextensions.k8s.io",
    "apiregistration.k8s.io",
    "apps",
    "authentication.k8s.io",
    "authorization.k8s.io",
    "autoscaling",
    "batch",
    "certificates.k8s.io",
    "coordination.k8s.io",
    "discovery.k8s.io",
    "events.k8s.io",
    "flowcontrol.apiserver.k8s.io",
    "internal.apiserver.k8s.io",
    "networking.k8s.io",
    "node.k8s.io",
    "policy",
    "rbac.authorization.k8s.io",
    "resource.k8s.io",
    "scheduling.k8s.io",
    "storage.k8s.io",
    "storagemigration.k8s.io",
];

/// Whether the Kubernetes API server itself defines `group`.
fn is_builtin_group(group: &str) -> bool {
    BUILTIN_GROUPS.contains(&group)
}

#[cfg(test)]
mod tests;
