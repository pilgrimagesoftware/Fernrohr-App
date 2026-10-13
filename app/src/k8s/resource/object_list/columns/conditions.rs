//! The generic condition columns (`resource-status-message` D2): Ready and
//! Message, for a kind outside the per-kind table - every CRD, Flux's
//! `HelmRelease` and `Kustomization` among them. Every row of such a kind
//! carries both cells; the list shows each column only once a loaded object
//! has reported what fills it, so a kind whose objects never set
//! `status.conditions` keeps its base columns.

use super::{Cell, ColumnDef, KindColumns};
use crate::k8s::resource::status_tone;
use kube::api::DynamicObject;

/// Each column's index in [`CONDITIONS`]' cells.
const READY: usize = 0;
const MESSAGE: usize = 1;

pub(super) static CONDITIONS: KindColumns = KindColumns {
    columns: &[
        ColumnDef {
            id: "ready",
            title: "Ready",
            width: 90.,
        },
        ColumnDef {
            id: "message",
            title: "Message",
            width: 360.,
        },
    ],
    cells,
};

fn cells(object: &DynamicObject) -> Vec<Cell> {
    let conditions = status_tone::conditions_of(&object.data);
    let mut cells = vec![Cell::Empty; CONDITIONS.columns.len()];
    if let Some(status) = status_tone::ready_status(conditions) {
        cells[READY] = Cell::Condition(status);
    }
    if let Some((message, tone)) = status_tone::condition_message(conditions) {
        cells[MESSAGE] = Cell::status(message, tone);
    }
    cells
}

/// What one or more objects have reported in `status.conditions` - what
/// decides which condition columns a list shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConditionsSeen {
    /// A non-empty `status.conditions`: the Message column shows.
    pub conditions: bool,
    /// A `Ready` condition among them: the Ready column shows too.
    pub ready: bool,
}

impl ConditionsSeen {
    /// What `object` reports.
    pub fn of(object: &DynamicObject) -> Self {
        let conditions = status_tone::conditions_of(&object.data);
        Self {
            conditions: !conditions.is_empty(),
            ready: status_tone::ready_status(conditions).is_some(),
        }
    }

    /// What either `self` or `other` reported.
    pub fn or(self, other: Self) -> Self {
        Self {
            conditions: self.conditions || other.conditions,
            ready: self.ready || other.ready,
        }
    }
}

/// The condition columns `seen` calls for, with their cell indices: Ready
/// before Message, and neither until conditions have been seen.
pub(super) fn visible(seen: ConditionsSeen) -> impl Iterator<Item = (usize, ColumnDef)> {
    CONDITIONS
        .columns
        .iter()
        .copied()
        .enumerate()
        .filter(move |(cell, _)| match *cell {
            READY => seen.ready,
            _ => seen.conditions,
        })
}
