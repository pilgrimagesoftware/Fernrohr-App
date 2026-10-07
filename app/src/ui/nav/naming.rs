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
            NavTarget::Exec(_) => "Shell".to_string(),
        }
    }

    /// What a panel *listing* this target titles itself: the plural form for
    /// a resource kind (`"Pods"`), same as [`Self::label`] for anything that
    /// isn't a list of many items. A custom resource's plural stands alone
    /// (`"Certificates"`); [`Self::custom_group`] carries its group to the
    /// tab's tooltip instead.
    pub fn list_label(&self) -> String {
        match self {
            NavTarget::Kind(kind) if self.custom_group().is_some() => kind.plural_name(),
            NavTarget::Kind(kind) => kind.plural_label(),
            NavTarget::Logs
            | NavTarget::PodLogs(_)
            | NavTarget::LabelLogs(_)
            | NavTarget::Pod(_)
            | NavTarget::Object(_)
            | NavTarget::Exec(_) => self.label(),
        }
    }

    /// The API group of a custom resource kind's list - a kind outside the
    /// built-in API groups - and `None` for every other target. The core
    /// group (`""`) never counts: a core kind the taxonomy hasn't caught up
    /// with has no group to show.
    pub fn custom_group(&self) -> Option<&str> {
        match self {
            NavTarget::Kind(kind)
                if !kind.gvk.group.is_empty()
                    && !crate::ui::panel::resource::is_built_in(&kind.gvk.group, &kind.plural) =>
            {
                Some(&kind.gvk.group)
            }
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
            NavTarget::Exec(exec) => format!("Shell: {} · {}", exec.pod, exec.container),
            _ => self.list_label(),
        }
    }
}
