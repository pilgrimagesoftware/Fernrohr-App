//! Which icon stands for a resource kind (`resource-icons` spec): the
//! Kubernetes community icon set's full-colour artwork for every built-in kind
//! it covers, and a fallback in the same style for everything else - so no
//! kind is ever drawn without one.
//!
//! The set's files live in `assets/icons/kubernetes/` (Apache-2.0, see the
//! `LICENSE` and `README.md` there); the three fallbacks in
//! `assets/icons/fallback/` are Fernrohr's own. This module only decides
//! which icon; drawing it is the views' business.

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
    /// Every icon, in declaration order.
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

#[cfg(test)]
mod tests;
