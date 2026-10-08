//! What a Pods table column is: its identity, title and default width, how two
//! rows compare on it, and the text it shows for a row. The delegate in the
//! parent module renders, sorts and reorders from this.

use super::super::pods::PodRow;
use std::cmp::Ordering;

/// One column of the Pods table. A closed enum rather than a string/index
/// pair: a `match` on it has no wildcard arm, so a new variant fails to
/// compile everywhere it isn't handled instead of silently falling back to
/// "Name" the way the old string-keyed columns did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::k8s::resource) enum PodColumn {
    Name,
    Namespace,
    Ready,
    Status,
    Restarts,
    Age,
    Ip,
    Node,
    /// `port-forward-indicators` 2.1: the forwards reaching the pod.
    Forwards,
}

impl PodColumn {
    /// Left-to-right order a freshly created table starts with.
    pub(in crate::k8s::resource) const DEFAULT_ORDER: [PodColumn; 9] = [
        PodColumn::Name,
        PodColumn::Namespace,
        PodColumn::Ready,
        PodColumn::Status,
        PodColumn::Forwards,
        PodColumn::Restarts,
        PodColumn::Age,
        PodColumn::Ip,
        PodColumn::Node,
    ];

    /// The column's stable id, used as both the `gpui_kit` [`Column`] key and
    /// [`crate::config::workspace::SortState::column`]'s persisted value.
    pub(super) fn id(self) -> &'static str {
        match self {
            PodColumn::Name => "name",
            PodColumn::Namespace => "namespace",
            PodColumn::Ready => "ready",
            PodColumn::Status => "status",
            PodColumn::Restarts => "restarts",
            PodColumn::Age => "age",
            PodColumn::Ip => "ip",
            PodColumn::Node => "node",
            PodColumn::Forwards => "forwards",
        }
    }

    pub(super) fn title(self) -> &'static str {
        match self {
            PodColumn::Name => "Name",
            PodColumn::Namespace => "Namespace",
            PodColumn::Ready => "Ready",
            PodColumn::Status => "Status",
            PodColumn::Restarts => "Restarts",
            PodColumn::Age => "Age",
            PodColumn::Ip => "IP",
            PodColumn::Node => "Node",
            PodColumn::Forwards => "Forwards",
        }
    }

    pub(super) fn default_width(self) -> f32 {
        match self {
            // Sized for a typical Deployment pod name - deployment, ReplicaSet
            // hash and pod suffix, like `checkout-service-7d9f8b6c5d-x2k9p`.
            PodColumn::Name => 320.,
            PodColumn::Namespace => 150.,
            PodColumn::Ready => 80.,
            PodColumn::Status => 130.,
            PodColumn::Restarts => 90.,
            PodColumn::Age => 70.,
            PodColumn::Ip => 150.,
            PodColumn::Node => 180.,
            PodColumn::Forwards => 90.,
        }
    }
}

/// The pure per-column comparison, shared by [`PodTableDelegate`]'s
/// interactive sort and `pods::sort_rows`'s view-pipeline sort - one
/// definition of "how does column X order two rows" for both.
pub(in crate::k8s::resource) fn compare(a: &PodRow, b: &PodRow, col: PodColumn) -> Ordering {
    match col {
        PodColumn::Name => a.name.cmp(&b.name),
        PodColumn::Namespace => a.namespace.cmp(&b.namespace),
        PodColumn::Ready => a.ready.cmp(&b.ready),
        PodColumn::Status => a.status.cmp(&b.status),
        // Raw seconds, not the display string: see `PodRow::age_secs`.
        PodColumn::Age => a.age_secs.cmp(&b.age_secs),
        PodColumn::Restarts => a.restarts.cmp(&b.restarts),
        PodColumn::Ip => a.pod_ip.cmp(&b.pod_ip),
        PodColumn::Node => a.node.cmp(&b.node),
        PodColumn::Forwards => a.forwards.cmp(&b.forwards),
    }
}

/// A column's rendered text for one row - the pure half of `render_td`.
pub(super) fn cell_text(row: &PodRow, col: PodColumn) -> String {
    match col {
        PodColumn::Name => row.name.clone(),
        PodColumn::Namespace => row.namespace.clone(),
        PodColumn::Ready => row.ready.clone(),
        PodColumn::Status => row.status.clone(),
        PodColumn::Restarts => row.restarts.to_string(),
        PodColumn::Age => row.age.clone(),
        PodColumn::Ip => row.pod_ip.clone(),
        PodColumn::Node => row.node.clone(),
        PodColumn::Forwards if row.forwards == 0 => String::new(),
        PodColumn::Forwards => row.forwards.to_string(),
    }
}
