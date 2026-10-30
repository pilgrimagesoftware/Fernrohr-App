//! The Pods panel's part in bulk actions (`bulk-select-list-actions`): its
//! checked rows as targets, and the pods they select, for View Logs.

use super::PodsPanel;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::bulk::BulkTarget;
use crate::ui::bulk::{BatchOutcome, BulkHost, BulkSelection, CheckedRows};
use gpui_kit::*;

impl BulkHost for PodsPanel {
    fn bulk_selection(&self, cx: &App) -> Option<BulkSelection> {
        let table = self.pod_table.as_ref()?.read(cx);
        let rows = table.delegate().checked_rows();
        if rows.is_empty() {
            return None;
        }
        let pods = self.table.read(cx).pods();
        let uid = |namespace: &str, name: &str| {
            pods.iter()
                .find(|pod| {
                    pod.metadata.namespace.as_deref() == Some(namespace)
                        && pod.metadata.name.as_deref() == Some(name)
                })
                .and_then(|pod| pod.metadata.uid.clone())
        };
        Some(BulkSelection {
            context_name: self.scope.context_name.clone(),
            connection: self.connection.clone(),
            targets: rows
                .iter()
                .map(|row| BulkTarget {
                    kind: DiscoveredKind::pods(),
                    namespace: Some(row.selection.namespace.clone()),
                    name: row.selection.name.clone(),
                    uid: uid(&row.selection.namespace, &row.selection.name),
                })
                .collect(),
            pods: rows.iter().map(|row| row.selection.clone()).collect(),
        })
    }

    fn edit_checked(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut CheckedRows)) {
        if let Some(table) = &self.pod_table {
            table.update(cx, |table, cx| {
                edit(table.delegate_mut().checked_mut());
                cx.notify();
            });
        }
        cx.notify();
    }

    fn check_cursor_row(&mut self, range: bool, cx: &mut Context<Self>) {
        if let Some(table) = &self.pod_table {
            table.update(cx, |table, cx| {
                if let Some(row_ix) = table.selected_row() {
                    table.delegate_mut().check_row(row_ix, range);
                    cx.notify();
                }
            });
        }
        cx.notify();
    }

    fn check_all_rows(&mut self, cx: &mut Context<Self>) {
        if let Some(table) = &self.pod_table {
            table.update(cx, |table, cx| {
                table.delegate_mut().check_all();
                cx.notify();
            });
        }
        cx.notify();
    }

    fn bulk_outcome(&mut self) -> &mut Option<BatchOutcome> {
        &mut self.bulk_outcome
    }
}
impl PodsPanel {
    /// Test-only: how many rows the table shows.
    #[cfg(test)]
    pub(crate) fn test_rows(&self, cx: &App) -> usize {
        self.pod_table
            .as_ref()
            .map_or(0, |table| table.read(cx).delegate().rows().len())
    }
}

#[cfg(test)]
mod tests;
