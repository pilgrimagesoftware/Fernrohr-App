//! How a [`NavTarget`] names itself: its constructors and the labels its
//! panels' titles and tabs read. What a target *is* stays in `nav`.

use super::*;

impl NavTarget {
    /// The target of the `nav.show_pods` command. Built without discovery, so
    /// it is pinned to the core `v1` `Pod` kind every cluster reports.
    pub fn pods() -> Self {
        NavTarget::Kind(DiscoveredKind::pods())
    }

    /// The detail view over one pod.
    pub fn pod(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        NavTarget::Pod(PodRef {
            namespace: namespace.into(),
            name: name.into(),
        })
    }

    /// The kind this target shows, for the cases that read it as one. A pod's
    /// detail panel is over the core `Pod` kind, pinned the same way
    /// [`Self::pods`] pins the list's.
    fn pod_kind() -> DiscoveredKind {
        DiscoveredKind::pods()
    }

    pub fn label(&self) -> String {
        match self {
            NavTarget::Kind(kind) => kind.label(),
            NavTarget::Logs | NavTarget::PodLogs(_) | NavTarget::LabelLogs(_) => "Logs".to_string(),
            NavTarget::Pod(_) => Self::pod_kind().label(),
            NavTarget::Object(object) => object.kind.label(),
            NavTarget::Exec(exec) => exec
                .command
                .as_ref()
                .map_or_else(|| "Shell".to_string(), |command| command.entry_name.clone()),
        }
    }

    /// What a panel *listing* this target titles itself: the plural form for
    /// a resource kind (`"Pods"`, `"HTTPRoutes"`), same as [`Self::label`] for
    /// anything that isn't a list of many items. The plural stands alone
    /// whatever the kind's API group (#206); [`Self::api_group`] carries the
    /// group to the tab's tooltip instead.
    pub fn list_label(&self) -> String {
        match self {
            NavTarget::Kind(kind) => kind.plural_name(),
            NavTarget::Logs
            | NavTarget::PodLogs(_)
            | NavTarget::LabelLogs(_)
            | NavTarget::Pod(_)
            | NavTarget::Object(_)
            | NavTarget::Exec(_) => self.label(),
        }
    }

    /// The API group of a kind's list outside the core group (`apps`,
    /// `gateway.networking.k8s.io`, a custom resource's), which tells two
    /// groups' same-named kinds apart in the tab's tooltip, and `None` for the
    /// core group and every other target.
    pub fn api_group(&self) -> Option<&str> {
        match self {
            NavTarget::Kind(kind) if !kind.gvk.group.is_empty() => Some(&kind.gvk.group),
            NavTarget::Kind(_)
            | NavTarget::Logs
            | NavTarget::PodLogs(_)
            | NavTarget::LabelLogs(_)
            | NavTarget::Pod(_)
            | NavTarget::Object(_)
            | NavTarget::Exec(_) => None,
        }
    }

    /// What a panel titles itself when it shows *one* item rather than a list of
    /// them: the kind's singular name, plus the item's own name so two panels
    /// over different pods are told apart in the dock's tabs.
    pub fn item_label(&self) -> String {
        match self {
            NavTarget::Pod(pod) => format!("{}: {}", self.label(), pod.name),
            NavTarget::PodLogs(pod) => format!("Logs: {}", pod.name),
            NavTarget::LabelLogs(source) => source.label(),
            NavTarget::Object(object) => format!("{}: {}", object.kind.gvk.kind, object.name),
            NavTarget::Exec(exec) => exec.title(),
            _ => self.list_label(),
        }
    }
}
