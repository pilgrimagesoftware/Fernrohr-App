//! A list panel's part in bulk actions (`bulk-select-list-actions`): its
//! checked rows as targets of the panel's one kind.

use super::ObjectListPanel;
use crate::k8s::resource::bulk::BulkTarget;
use crate::ui::bulk::{BatchOutcome, BulkHost, BulkSelection, CheckedRows};
use gpui_kit::*;

impl BulkHost for ObjectListPanel {
    fn bulk_selection(&self, cx: &App) -> Option<BulkSelection> {
        let table = self.table.as_ref()?.read(cx);
        let rows = table.delegate().checked_rows();
        if rows.is_empty() {
            return None;
        }
        Some(BulkSelection {
            context_name: self.scope.context_name.clone(),
            connection: self.connection.clone(),
            targets: rows
                .iter()
                .map(|row| {
                    let key = row.row_key();
                    BulkTarget {
                        kind: self.kind.clone(),
                        namespace: key.namespace,
                        name: key.name,
                        uid: (!row.object.uid.is_empty()).then(|| row.object.uid.clone()),
                    }
                })
                .collect(),
            pods: Vec::new(),
        })
    }

    fn edit_checked(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut CheckedRows)) {
        if let Some(table) = &self.table {
            table.update(cx, |table, cx| {
                edit(table.delegate_mut().checked_mut());
                cx.notify();
            });
        }
        cx.notify();
    }

    fn check_cursor_row(&mut self, range: bool, cx: &mut Context<Self>) {
        if let Some(table) = &self.table {
            table.update(cx, |table, cx| {
                if let Some(row_ix) = table.selected_row() {
                    table.delegate_mut().check_row(row_ix, range);
                    cx.notify();
                }
            });
        }
        cx.notify();
    }

    fn bulk_outcome(&mut self) -> &mut Option<BatchOutcome> {
        &mut self.bulk_outcome
    }
}

#[cfg(test)]
mod tests;
