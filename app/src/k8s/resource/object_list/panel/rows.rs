//! The rows and columns a list panel shows now: its kind's objects narrowed
//! to its namespaces and filter, and its default columns - which, for a kind
//! outside the per-kind table, follow the conditions its objects report
//! (`resource-status-message`). Drawing them is `render`'s.

use super::ObjectListPanel;
use crate::k8s::resource::object_list::table::{ListColumn, ListRow, visible_texts};
use crate::ui::list_search;
use gpui_kit::App;

impl ObjectListPanel {
    /// The kind's columns in their default order, given what its loaded
    /// objects have reported - a kind outside the per-kind table gains its
    /// Ready and Message columns once they report conditions.
    pub(in crate::k8s::resource::object_list) fn default_columns(
        &self,
        cx: &App,
    ) -> Vec<ListColumn> {
        ListColumn::for_kind(&self.kind, self.objects.read(cx).conditions_seen())
    }

    /// The rows to show now: the kind's objects in this panel's namespaces
    /// whose visible column text - every one of [`Self::default_columns`],
    /// the table's own column set regardless of the order the user left them
    /// in - contains the filter text, case-insensitively (`list-search` #189).
    pub(in crate::k8s::resource::object_list) fn visible_rows(&self, cx: &App) -> Vec<ListRow> {
        let now = jiff::Timestamp::now();
        let namespaces = &self.scope.namespaces;
        let query = self.filter.query(cx);
        let columns = self.default_columns(cx);
        self.objects
            .read(cx)
            .rows()
            .iter()
            .filter(|row| {
                namespaces.is_empty()
                    || row
                        .namespace
                        .as_ref()
                        .is_some_and(|namespace| namespaces.contains(namespace))
            })
            .map(|row| {
                let mut listed = ListRow::new(row.clone(), now);
                listed.forwards = self.forwards_of(row, cx);
                listed
            })
            .filter(|row| list_search::matches(visible_texts(row, &columns), &query))
            .collect()
    }
}
