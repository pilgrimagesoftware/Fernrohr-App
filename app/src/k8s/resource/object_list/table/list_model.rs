//! What a list table's column and row are, and how two rows compare on one
//! (split out of `table.rs` to keep that file under the 500-line limit -
//! `table.rs` owns the live [`super::ObjectTableDelegate`] built from these;
//! this owns their plain data and comparisons).
//!
//! Columns are identified by a string key, not a position or a closed enum
//! (`standard-resource-panels` D2), so a saved column order and widths survive a
//! kind gaining columns, and the per-kind columns join the base ones without a
//! new type per kind. Every kind has the base columns - Name, Namespace for a
//! namespaced kind, and Age - and a built-in kind its own between them
//! ([`super::super::columns`]).

use std::cmp::Ordering;

use gpui_kit::*;
use serde::{Deserialize, Serialize};

use super::super::columns;
use super::super::row::ObjectRow;
use crate::k8s::cluster::discovery::DiscoveredKind;

/// The base columns' keys. Also what [`ColumnLayout`] saves.
pub(super) const NAME: &str = "name";
pub(super) const NAMESPACE: &str = "namespace";
pub(super) const AGE: &str = "age";
/// A core Service list's Forwards column (`port-forward-indicators` 2.1).
pub(super) const FORWARDS: &str = "forwards";

/// One column of a list table.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::k8s::resource::object_list) struct ListColumn {
    pub(in crate::k8s::resource::object_list) id: SharedString,
    pub(super) title: SharedString,
    pub(in crate::k8s::resource::object_list) width: Pixels,
    /// For one of the kind's own columns, which of a row's
    /// [`ObjectRow::cells`] it shows; `None` for a base column.
    pub(super) cell: Option<usize>,
}

impl ListColumn {
    fn new(id: &'static str, title: &'static str, width: f32) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            width: px(width),
            cell: None,
        }
    }

    /// `kind`'s columns in their default order, as `kubectl get` lays them
    /// out: Name, Namespace for a namespaced kind only, the kind's own columns,
    /// then Age.
    pub(in crate::k8s::resource::object_list) fn for_kind(
        kind: &DiscoveredKind,
    ) -> Vec<ListColumn> {
        let mut columns = vec![ListColumn::new(NAME, "Name", 260.)];
        if kind.namespaced {
            columns.push(ListColumn::new(NAMESPACE, "Namespace", 150.));
        }
        if let Some(own) = columns::for_kind(&kind.gvk.group, &kind.gvk.kind) {
            columns.extend(
                own.columns
                    .iter()
                    .enumerate()
                    .map(|(cell, def)| ListColumn {
                        cell: Some(cell),
                        ..ListColumn::new(def.id, def.title, def.width)
                    }),
            );
        }
        if kind.gvk.group.is_empty() && kind.gvk.kind == "Service" {
            columns.push(ListColumn::new(FORWARDS, "Forwards", 90.));
        }
        columns.push(ListColumn::new(AGE, "Age", 70.));
        columns
    }
}

/// How one column is laid out: what a panel saves so its columns come back in the
/// order and at the widths the user left them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(in crate::k8s::resource::object_list) struct SavedColumn {
    pub(in crate::k8s::resource::object_list) id: String,
    pub(in crate::k8s::resource::object_list) width: f32,
}

/// A table's column layout, left to right.
pub(in crate::k8s::resource::object_list) type ColumnLayout = Vec<SavedColumn>;

/// `columns` rearranged and resized to `saved`: the saved columns first, in their
/// saved order and widths, then any column `saved` doesn't mention in its default
/// place. A saved column the kind no longer has is dropped.
pub(in crate::k8s::resource::object_list) fn apply_layout(
    columns: Vec<ListColumn>,
    saved: &[SavedColumn],
) -> Vec<ListColumn> {
    let mut remaining = columns;
    let mut ordered = Vec::with_capacity(remaining.len());
    for saved in saved {
        if let Some(position) = remaining.iter().position(|column| column.id == saved.id) {
            let mut column = remaining.remove(position);
            column.width = px(saved.width);
            ordered.push(column);
        }
    }
    ordered.extend(remaining);
    ordered
}

/// One listed object as the table shows it: the row, its age now, and the
/// moment that "now" was - taken once per refresh, so every time-like cell's
/// sort and text agree.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::k8s::resource::object_list) struct ListRow {
    pub(in crate::k8s::resource::object_list) object: ObjectRow,
    pub(in crate::k8s::resource::object_list) age_secs: i64,
    pub(super) now: jiff::Timestamp,
    /// For a Service, the forwards started from it - its Forwards cell.
    pub(in crate::k8s::resource::object_list) forwards:
        Vec<crate::k8s::cluster::port_forwards::ForwardSummary>,
}

impl ListRow {
    pub(in crate::k8s::resource::object_list) fn new(
        object: ObjectRow,
        now: jiff::Timestamp,
    ) -> Self {
        let age_secs = object.age_secs(now);
        Self {
            object,
            age_secs,
            now,
            forwards: Vec::new(),
        }
    }

    /// The row's cell for `column`, if it is one of the kind's own columns.
    pub(super) fn cell(&self, column: &ListColumn) -> Option<&columns::Cell> {
        self.object.cells.get(column.cell?)
    }

    /// The object's identity within its kind: namespace (if any) and name. What
    /// selection follows across sorts and watch updates.
    pub(super) fn key(&self) -> (Option<&str>, &str) {
        (self.object.namespace.as_deref(), &self.object.name)
    }
}

/// How `a` and `b` order on `column`. Age sorts by seconds, and a kind's own
/// column by its cells' values - numerically where they are numbers.
pub(super) fn compare(a: &ListRow, b: &ListRow, column: &ListColumn) -> Ordering {
    if column.cell.is_some() {
        let empty = columns::Cell::Empty;
        let (a_cell, b_cell) = (
            a.cell(column).unwrap_or(&empty),
            b.cell(column).unwrap_or(&empty),
        );
        return a_cell.compare(b_cell, a.now);
    }
    match column.id.as_ref() {
        NAME => a.object.name.cmp(&b.object.name),
        NAMESPACE => a.object.namespace.cmp(&b.object.namespace),
        AGE => a.age_secs.cmp(&b.age_secs),
        FORWARDS => a.forwards.len().cmp(&b.forwards.len()),
        // Every column `ListColumn::for_kind` makes is handled above; a key from
        // elsewhere (a hand-edited layout) has nothing to compare by.
        _ => Ordering::Equal,
    }
}

/// `column`'s text for `row`.
pub(super) fn cell_text(row: &ListRow, column: &ListColumn) -> String {
    if column.cell.is_some() {
        return row
            .cell(column)
            .map(|cell| cell.display(row.now))
            .unwrap_or_default();
    }
    match column.id.as_ref() {
        NAME => row.object.name.clone(),
        NAMESPACE => row.object.namespace.clone().unwrap_or_default(),
        AGE => crate::k8s::resource::pods::format_age(row.age_secs),
        FORWARDS if row.forwards.is_empty() => String::new(),
        FORWARDS => row.forwards.len().to_string(),
        _ => String::new(),
    }
}
