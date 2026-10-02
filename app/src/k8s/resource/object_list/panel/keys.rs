//! The list's own keys beyond Enter and `/`: describe and YAML
//! (`standard-resource-panels` 5.1) - `d` and `y` on the selected row, as the Pods
//! table answers them - and Up/Down from no selection (5.2, [`list_keys`]).

use super::ObjectListPanel;
use crate::k8s::resource::object_list::commands::{DescribeSelected, ShowSelectedYaml};
use crate::k8s::resource::pod_detail::DetailView;
use crate::ui::list_keys::{self, Step};
use gpui_kit::base::actions::{SelectDown, SelectUp};
use gpui_kit::{Context, Window};

impl ObjectListPanel {
    pub(in crate::k8s::resource::object_list) fn on_action_describe_selected(
        &mut self,
        _: &DescribeSelected,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(row_ix) = self.selected_row(cx) {
            self.open_row_as(row_ix, Some(DetailView::Structured), window, cx);
        }
    }

    pub(in crate::k8s::resource::object_list) fn on_action_show_selected_yaml(
        &mut self,
        _: &ShowSelectedYaml,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(row_ix) = self.selected_row(cx) {
            self.open_row_as(row_ix, Some(DetailView::Yaml), window, cx);
        }
    }

    /// The table's selected row. Test-only.
    #[cfg(test)]
    pub(crate) fn test_selected_row(&self, cx: &gpui_kit::App) -> Option<usize> {
        self.selected_row(cx)
    }

    pub(in crate::k8s::resource::object_list) fn capture_select_down(
        &mut self,
        _: &SelectDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(table) = self.table.clone()
            && list_keys::step(&table, Step::Down, window, cx)
        {
            cx.stop_propagation();
        }
    }

    pub(in crate::k8s::resource::object_list) fn capture_select_up(
        &mut self,
        _: &SelectUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(table) = self.table.clone()
            && list_keys::step(&table, Step::Up, window, cx)
        {
            cx.stop_propagation();
        }
    }
}
