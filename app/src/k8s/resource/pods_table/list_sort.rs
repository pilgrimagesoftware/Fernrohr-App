//! A Pods table's part in `ui::list_sort`: its columns by id, its default sort
//! (Name, ascending, the first of [`PodColumn::DEFAULT_ORDER`]), and the key
//! its user's sorts are remembered under.

use super::{PodColumn, PodTableDelegate, reselect};
use crate::ui::list_sort::{Sort, SortableTable};
use gpui_kit::component::table::TableState;
use gpui_kit::{Context, SharedString};

/// What a third header click or Cycle Sort returns a Pods table to.
pub(in crate::k8s::resource) fn default_sort() -> Sort {
    (SharedString::from(PodColumn::DEFAULT_ORDER[0].id()), false)
}

/// Every Pods column's id: the columns a saved or remembered sort may name.
pub(in crate::k8s::resource) fn column_ids() -> Vec<SharedString> {
    PodColumn::DEFAULT_ORDER
        .iter()
        .map(|column| SharedString::from(column.id()))
        .collect()
}

impl PodTableDelegate {
    /// Remembers the user's sorts under `key`.
    pub(in crate::k8s::resource) fn remember_as(&mut self, key: String) {
        self.remember_as = Some(key);
    }
}

impl SortableTable for PodTableDelegate {
    fn sort_columns(&self) -> Vec<SharedString> {
        self.columns
            .iter()
            .map(|column| SharedString::from(column.id()))
            .collect()
    }

    fn current_sort(&self) -> Option<Sort> {
        self.sort_state()
            .map(|(column, descending)| (SharedString::from(column), descending))
    }

    fn sort_by(&mut self, (column, descending): &Sort) {
        self.set_sort_state(column, *descending);
    }

    fn default_sort(&self) -> Sort {
        default_sort()
    }

    fn remember_as(&self) -> Option<&str> {
        self.remember_as.as_deref()
    }

    fn after_sort(table: &mut TableState<Self>, cx: &mut Context<TableState<Self>>) {
        reselect(table, cx);
    }
}
