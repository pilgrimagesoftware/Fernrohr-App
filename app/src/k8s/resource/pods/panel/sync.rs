//! Handing the Pods panel's rows to its table: building the table the first
//! time, with its row events wired, then setting the rows each render.

use super::*;

impl PodsPanel {
    pub(in crate::k8s::resource::pods) fn sync_table(
        &mut self,
        rows: Vec<PodTableRow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TableState<PodTableDelegate>> {
        if self.pod_table.is_none() {
            let key = crate::ui::list_sort::kind_key(&DiscoveredKind::pods());
            let ids = crate::k8s::resource::pods_table::list_sort::column_ids();
            let start = crate::ui::list_sort::starting(
                self.initial_sort
                    .clone()
                    .map(|(column, descending)| (column.into(), descending)),
                crate::util::shell::SortDefaults::get(cx, &key),
                &ids,
                crate::k8s::resource::pods_table::list_sort::default_sort(),
            );
            let table = cx.new(|cx| {
                let mut delegate = PodTableDelegate::default();
                delegate.remember_as(key);
                delegate.set_sort_state(&start.0, start.1);
                TableState::new(delegate, window, cx)
                    .row_selectable(true)
                    .col_selectable(false)
                    .sortable(true)
                    .col_movable(true)
                    .col_resizable(true)
            });
            // The row context menu dispatches its commands from the table, so
            // they reach this panel's handlers as their keys do.
            table.update(cx, |table, cx| {
                let focus = table.focus_handle(cx);
                table.delegate_mut().set_action_context(focus);
            });
            cx.subscribe_in(&table, window, |this, table, event, window, cx| {
                if let TableEvent::ColumnWidthsChanged(widths) = event {
                    table.update(cx, |table, _| table.delegate_mut().set_widths(widths));
                    return;
                }
                let row_ix = match event {
                    // A modified click's selection is put back, and nothing
                    // follows the clicked row (`ui::background_rows`).
                    TableEvent::SelectRow(_)
                        if table.update(cx, |table, cx| {
                            crate::ui::background_rows::undo_select(table, cx)
                        }) =>
                    {
                        return;
                    }
                    TableEvent::DoubleClickedRow(_)
                        if table.update(cx, |table, _| {
                            crate::ui::background_rows::swallow_double_click(table)
                        }) =>
                    {
                        return;
                    }
                    TableEvent::SelectRow(row_ix) => *row_ix,
                    // A single click only selects (drives WarpNamespace/
                    // ShowPodLogs, which read SelectedPod); opening the
                    // detail panel is the double-click, matching the
                    // resource panel's own row convention (section 9.1).
                    TableEvent::DoubleClickedRow(row_ix) => {
                        let Some(selection) = table
                            .read(cx)
                            .delegate()
                            .rows()
                            .get(*row_ix)
                            .map(|row| row.selection.clone())
                        else {
                            return;
                        };
                        remember_selection(table, &selection, cx);
                        cx.set_global(SelectedPod(Some(selection)));
                        window.dispatch_action(Box::new(crate::ui::nav::ShowPodDetail), cx);
                        return;
                    }
                    _ => return,
                };
                let Some(selection) = table
                    .read(cx)
                    .delegate()
                    .rows()
                    .get(row_ix)
                    .map(|row| row.selection.clone())
                else {
                    return;
                };
                remember_selection(table, &selection, cx);
                this.retarget_quick_look(&selection, cx);
                cx.set_global(SelectedPod(Some(selection)));
                cx.notify();
            })
            .detach();
            self.pod_table = Some(table);
        }
        let table = self.pod_table.as_ref().unwrap().clone();
        table.update(cx, |table, cx| {
            table.delegate_mut().set_rows(rows);
            // A row update can move or drop the pod at the previously
            // selected index (see `pods_table::reselect`'s doc comment) -
            // called directly, not deferred, since `table` here is already
            // the full `&mut TableState`, unlike inside `perform_sort`.
            pods_table::reselect(table, cx);
            cx.notify();
        });
        table
    }
}
