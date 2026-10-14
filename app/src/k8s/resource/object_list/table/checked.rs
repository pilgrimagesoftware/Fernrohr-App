//! A list panel's table's checked rows (`bulk-select-list-actions`): the checkbox in
//! each row's first cell, toggling and range-checking from it or from the
//! panel's `x`/Shift+`x`, and the checked objects in displayed order.

use super::{ListRow, ObjectTableDelegate};
use crate::ui::bulk::{CheckedRows, RowKey};
use gpui_kit::component::table::TableState;
use gpui_kit::*;

impl ListRow {
    pub(in crate::k8s::resource) fn row_key(&self) -> RowKey {
        let (namespace, name) = self.key();
        RowKey::new(namespace, name)
    }
}

impl ObjectTableDelegate {
    pub(in crate::k8s::resource) fn checked_mut(&mut self) -> &mut CheckedRows {
        &mut self.checked
    }

    /// The displayed rows' keys, top to bottom.
    fn row_keys(&self) -> Vec<RowKey> {
        self.rows.iter().map(ListRow::row_key).collect()
    }

    /// The checked rows, top to bottom.
    pub(in crate::k8s::resource) fn checked_rows(&self) -> Vec<&ListRow> {
        self.rows
            .iter()
            .filter(|row| self.checked.contains(&row.row_key()))
            .collect()
    }

    /// Row `row_ix`'s checkbox clicked, or `x` pressed on it: toggles it, or -
    /// `range` - checks everything from the last toggled row to it.
    pub(in crate::k8s::resource) fn check_row(&mut self, row_ix: usize, range: bool) {
        let Some(key) = self.rows.get(row_ix).map(ListRow::row_key) else {
            return;
        };
        if range {
            let rows = self.row_keys();
            self.checked.check_range(&rows, &key);
        } else {
            self.checked.toggle(key);
        }
    }

    /// Unchecks the objects a watch update no longer lists.
    pub(super) fn prune_checked(&mut self) {
        let rows = self.row_keys();
        self.checked.retain_listed(&rows);
    }

    /// `cell`, with the row's checkbox before it when it is the first column.
    pub(super) fn with_checkbox(
        &self,
        row_ix: usize,
        col_ix: usize,
        cell: impl IntoElement,
        cx: &mut Context<TableState<Self>>,
    ) -> AnyElement {
        if col_ix != 0 {
            return cell.into_any_element();
        }
        let checked = self
            .rows
            .get(row_ix)
            .is_some_and(|row| self.checked.contains(&row.row_key()));
        let table = cx.entity().downgrade();
        crate::ui::bulk::with_checkbox("object", row_ix, checked, cell, move |range, _, cx| {
            let _ = table.update(cx, |table, cx| {
                table.delegate_mut().check_row(row_ix, range);
                cx.notify();
            });
        })
        .into_any_element()
    }
}
