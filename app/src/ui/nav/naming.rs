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
    /// unless another group's kind shares its name (#206,
    /// [`Self::item_label_qualified`]); [`Self::api_version`] carries the
    /// group to the tab's tooltip.
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

    /// The `group/version` of the kind this target shows (`"apps/v1"`, `"v1"`)
    /// for the title's tooltip, and `None` for a target that isn't one kind.
    pub fn api_version(&self) -> Option<String> {
        match self {
            NavTarget::Kind(kind) => Some(kind.api_version()),
            NavTarget::Object(object) => Some(object.kind.api_version()),
            NavTarget::Pod(_) => Some(Self::pod_kind().api_version()),
            NavTarget::Logs
            | NavTarget::PodLogs(_)
            | NavTarget::LabelLogs(_)
            | NavTarget::Exec(_) => None,
        }
    }

    /// [`Self::item_label`], with the kind's API group added where
    /// `shares_name` says another group's kind has its name (#206), so the
    /// two panels' titles differ. A core kind keeps its bare name.
    pub fn item_label_qualified(&self, shares_name: impl Fn(&DiscoveredKind) -> bool) -> String {
        match self {
            NavTarget::Kind(kind) if shares_name(kind) => kind.qualified(&kind.plural_name()),
            NavTarget::Object(object) if shares_name(&object.kind) => {
                format!("{}: {}", object.kind.label(), object.name)
            }
            _ => self.item_label(),
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
