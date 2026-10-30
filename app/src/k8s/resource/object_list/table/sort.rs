//! The list table's sort: what [`crate::ui::list_sort`] needs of it, and
//! keeping the selection on its object as rows move.

use super::ObjectTableDelegate;
use gpui_kit::component::table::TableState;
use gpui_kit::*;

impl crate::ui::list_sort::SortableTable for ObjectTableDelegate {
    fn sort_columns(&self) -> Vec<SharedString> {
        self.columns
            .iter()
            .map(|column| column.id.clone())
            .collect()
    }

    fn current_sort(&self) -> Option<crate::ui::list_sort::Sort> {
        self.sort_state()
            .map(|(column, descending)| (SharedString::from(column.to_string()), descending))
    }

    fn sort_by(&mut self, (column, descending): &crate::ui::list_sort::Sort) {
        self.set_sort(column, *descending);
    }

    fn default_sort(&self) -> crate::ui::list_sort::Sort {
        self.default_sort.clone()
    }

    fn remember_as(&self) -> Option<&str> {
        self.remember_as.as_deref()
    }

    fn after_sort(table: &mut TableState<Self>, cx: &mut Context<TableState<Self>>) {
        reselect(table, cx);
    }
}

/// Re-points the table's row selection - which [`TableState`] keeps as a bare
/// index - at the object this table last selected, after a sort or a watch update
/// changed what sits at each index. Clears it if the object is gone.
pub(in crate::k8s::resource::object_list) fn reselect(
    table: &mut TableState<ObjectTableDelegate>,
    cx: &mut Context<TableState<ObjectTableDelegate>>,
) {
    if table.delegate().selected.is_none() {
        return;
    }
    match table.delegate().selected_index() {
        Some(row_ix) if table.selected_row() != Some(row_ix) => table.set_selected_row(row_ix, cx),
        Some(_) => {}
        None if table.selected_row().is_some() => table.clear_selection(cx),
        None => {}
    }
}
