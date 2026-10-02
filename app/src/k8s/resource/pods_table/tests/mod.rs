//! The pod table's tests: sorting and column order, and keeping the selection
//! on the right pod as rows move.

// Named imports rather than `use super::*`: `gpui_kit::*` (imported by the
// parent) re-exports its own `test` attribute macro, which would shadow the
// built-in `#[test]` for these plain synchronous tests.
use crate::k8s::resource::pods::{PodRow, PodSelection};

mod fit;
mod fonts;
mod selection;
mod sorting;
mod spacing;

/// Four rows with a distinct, non-alphabetical value in every column, so
/// sorting by any one of them actually reorders the set rather than
/// leaving it looking like the input by coincidence.
pub(super) fn pod_table_rows_fixture() -> Vec<crate::k8s::resource::pods_table::PodTableRow> {
    use crate::k8s::resource::pods_table::PodTableRow;

    [
        (
            "b-name", "ns-c", "1/2", "Running", 3, 300, "10.0.0.3", "node-c",
        ),
        (
            "d-name", "ns-a", "2/2", "Pending", 1, 100, "10.0.0.1", "node-a",
        ),
        (
            "a-name", "ns-d", "0/2", "Failed", 4, 400, "10.0.0.4", "node-d",
        ),
        (
            "c-name",
            "ns-b",
            "3/3",
            "Succeeded",
            2,
            200,
            "10.0.0.2",
            "node-b",
        ),
    ]
    .into_iter()
    .map(
        |(name, namespace, ready, status, restarts, age_secs, ip, node)| PodTableRow {
            row: PodRow {
                status_tone: crate::ui::style::Tone::Neutral,
                ready_tone: crate::ui::style::Tone::Neutral,
                name: name.into(),
                namespace: namespace.into(),
                ready: ready.into(),
                status: status.into(),
                restarts,
                age: String::new(),
                pod_ip: ip.into(),
                node: node.into(),
                age_secs,
            },
            selection: PodSelection {
                namespace: namespace.into(),
                name: name.into(),
                containers: Vec::new(),
                context_name: "ctx".into(),
            },
        },
    )
    .collect()
}
