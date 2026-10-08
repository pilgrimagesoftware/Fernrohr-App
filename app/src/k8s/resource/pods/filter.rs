//! The Pods panel's filter box (`list-search` #189): `/` focuses it, typing
//! narrows the table to rows whose visible column text contains it,
//! case-insensitively, and Escape clears it and returns focus to the table -
//! the same mechanism Events and every other list panel shares
//! (`crate::ui::list_search`). What counts as "visible" here is the table's
//! own fixed column set (`pods_table::visible_texts`), gathered in `render`.

use super::*;

/// The filter box's placeholder - narrows rows by every visible column, not
/// name alone.
pub(super) const FILTER_PLACEHOLDER: &str = "Filter rows...";

impl PodsPanel {
    /// `FocusFilter` (`/`): focuses the filter box, building it first if this
    /// is the first time.
    pub(super) fn on_action_focus_filter(
        &mut self,
        _: &FocusFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.filter.focus(FILTER_PLACEHOLDER, window, cx);
    }

    /// Escape in the filter: clears it and hands focus back to the table, so
    /// the keyboard is never left stranded in the text field.
    pub(super) fn on_action_clear_filter(
        &mut self,
        _: &gpui_kit::component::input::Escape,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = self.table_focus_handle(cx);
        self.filter.clear(&focus, window, cx);
        cx.notify();
    }

    /// Focus to return to once the filter clears - the table if it's drawn,
    /// the panel itself otherwise.
    fn table_focus_handle(&self, cx: &App) -> FocusHandle {
        match &self.pod_table {
            Some(table) => table.read(cx).focus_handle(cx),
            None => self.focus_handle.clone(),
        }
    }

    /// Whether the filter box holds focus. Test-only.
    #[cfg(test)]
    pub(crate) fn filter_focused(&self, window: &Window, cx: &App) -> bool {
        self.filter.is_focused(window, cx)
    }
}

/// The filter text a saved Pods panel names, if any (`saved-panel-layouts`
/// 1.6) - absent/`null` for no saved filter, true of every layout saved
/// before this field existed.
pub(super) fn filter_from_state(state: &serde_json::Value) -> Option<String> {
    state["filter"].as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::filter_from_state;

    #[test]
    fn filter_from_state_reads_null_as_no_saved_filter() {
        assert_eq!(filter_from_state(&serde_json::json!({})), None);
        assert_eq!(
            filter_from_state(&serde_json::json!({ "filter": null })),
            None
        );
        assert_eq!(
            filter_from_state(&serde_json::json!({ "filter": "web" })),
            Some("web".to_string())
        );
    }
}
