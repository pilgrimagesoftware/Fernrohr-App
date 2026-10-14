//! Which bulk actions a checked set is offered: only the ones that apply to
//! every checked object's kind. The kind sets are `kubectl`'s own - what
//! `rollout restart`/`undo`, `scale`, `cordon`/`drain` and `logs` each accept.

use crate::k8s::cluster::discovery::DiscoveredKind;

/// Every bulk action, in the order the bulk action bar shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BulkAction {
    ViewLogs,
    RestartRollout,
    RollbackRollout,
    Scale,
    Cordon,
    Uncordon,
    Drain,
    Label,
    Annotate,
    CopyNames,
    CopyYaml,
    Delete,
}

/// A set of kinds that an action needs every checked object to belong to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KindGroup {
    /// Deployment, DaemonSet, StatefulSet: `kubectl rollout restart`/`undo`.
    Rollout,
    /// Deployment, StatefulSet, ReplicaSet: `kubectl scale`.
    Scalable,
    Node,
    Pod,
}

impl KindGroup {
    pub(crate) fn contains(self, kind: &DiscoveredKind) -> bool {
        let (group, kind) = (kind.gvk.group.as_str(), kind.gvk.kind.as_str());
        match self {
            Self::Rollout => {
                group == "apps" && matches!(kind, "Deployment" | "DaemonSet" | "StatefulSet")
            }
            Self::Scalable => {
                group == "apps" && matches!(kind, "Deployment" | "StatefulSet" | "ReplicaSet")
            }
            Self::Node => group.is_empty() && kind == "Node",
            Self::Pod => group.is_empty() && kind == "Pod",
        }
    }
}

/// What an action asks of the API server, so a kind that can't be deleted or
/// patched isn't offered an action every object would fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Needs {
    Nothing,
    Delete,
    Patch,
}

impl BulkAction {
    pub(crate) const ALL: [Self; 12] = [
        Self::ViewLogs,
        Self::RestartRollout,
        Self::RollbackRollout,
        Self::Scale,
        Self::Cordon,
        Self::Uncordon,
        Self::Drain,
        Self::Label,
        Self::Annotate,
        Self::CopyNames,
        Self::CopyYaml,
        Self::Delete,
    ];

    /// The bar's button label, and the verb its confirmation names.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::ViewLogs => "View Logs",
            Self::RestartRollout => "Restart Rollout",
            Self::RollbackRollout => "Rollback Rollout",
            Self::Scale => "Scale",
            Self::Cordon => "Cordon",
            Self::Uncordon => "Uncordon",
            Self::Drain => "Drain",
            Self::Label => "Label",
            Self::Annotate => "Annotate",
            Self::CopyNames => "Copy Names",
            Self::CopyYaml => "Copy YAML",
            Self::Delete => "Delete",
        }
    }

    /// The kinds every checked object must be, or `None` for any kind.
    pub(crate) fn group(self) -> Option<KindGroup> {
        match self {
            Self::ViewLogs => Some(KindGroup::Pod),
            Self::RestartRollout | Self::RollbackRollout => Some(KindGroup::Rollout),
            Self::Scale => Some(KindGroup::Scalable),
            Self::Cordon | Self::Uncordon | Self::Drain => Some(KindGroup::Node),
            Self::Label | Self::Annotate | Self::CopyNames | Self::CopyYaml | Self::Delete => None,
        }
    }

    fn needs(self) -> Needs {
        match self {
            Self::Delete => Needs::Delete,
            Self::RestartRollout
            | Self::RollbackRollout
            | Self::Scale
            | Self::Cordon
            | Self::Uncordon
            | Self::Drain
            | Self::Label
            | Self::Annotate => Needs::Patch,
            Self::ViewLogs | Self::CopyNames | Self::CopyYaml => Needs::Nothing,
        }
    }

    /// Whether this action applies to `kind`.
    pub(crate) fn applies_to(self, kind: &DiscoveredKind) -> bool {
        let in_group = self.group().is_none_or(|group| group.contains(kind));
        let allowed = match self.needs() {
            Needs::Nothing => true,
            Needs::Delete => kind.verbs.delete,
            Needs::Patch => kind.verbs.patch,
        };
        in_group && allowed
    }

    /// The actions offered for a checked set of `kinds`, in bar order: each one
    /// that applies to every kind. Nothing is offered for an empty set.
    pub(crate) fn offered<'a>(kinds: impl IntoIterator<Item = &'a DiscoveredKind>) -> Vec<Self> {
        let kinds: Vec<&DiscoveredKind> = kinds.into_iter().collect();
        if kinds.is_empty() {
            return Vec::new();
        }
        Self::ALL
            .into_iter()
            .filter(|action| kinds.iter().all(|kind| action.applies_to(kind)))
            .collect()
    }
}
