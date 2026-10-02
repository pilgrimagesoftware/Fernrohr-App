//! Saving and restoring an events browser with the window's dock layout: its
//! context, namespace selection and sort. [`dump`] and [`from_state`] own the
//! saved shape.

use super::columns::EventColumn;
use super::filters::EventFilters;
use super::panel::EventsPanel;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::base::dock::PanelView;
use gpui_kit::component::dock::{PanelInfo, panel_handle, register_panel};
use gpui_kit::component::table::ColumnSort;
use gpui_kit::*;
use serde_json::{Value, json};
use std::sync::Arc;

/// Registers the dock's restore for saved events browsers.
pub fn register_restore(cx: &mut App) {
    register_panel(cx, "Events", |context, _window, cx| {
        let PanelInfo::Panel(state) = context.info() else {
            panic!("Events layout state must be a panel");
        };
        restore(state, cx)
    });
}

/// One saved events browser, as [`dump`] wrote it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SavedEvents {
    pub(crate) context_name: String,
    pub(crate) namespaces: Vec<String>,
    pub(super) sort: Option<(EventColumn, ColumnSort)>,
    pub(super) filters: EventFilters,
}

fn sort_name(sort: ColumnSort) -> &'static str {
    match sort {
        ColumnSort::Ascending => "ascending",
        ColumnSort::Descending => "descending",
        ColumnSort::Default => "default",
    }
}

fn sort_from_name(name: &str) -> Option<ColumnSort> {
    match name {
        "ascending" => Some(ColumnSort::Ascending),
        "descending" => Some(ColumnSort::Descending),
        "default" => Some(ColumnSort::Default),
        _ => None,
    }
}

/// What `panel` saves.
pub(super) fn dump(panel: &EventsPanel, cx: &App) -> Value {
    let sort = panel
        .sort(cx)
        .map(|(column, sort)| json!({ "column": column.id(), "order": sort_name(sort) }));
    json!({
        "context_name": panel.scope.context_name,
        "namespaces": panel.scope.namespaces,
        "sort": sort,
        "filters": panel.filters,
    })
}

/// The browser a saved panel was, or `None` for state that doesn't name its
/// cluster. A sort naming a column or order this build doesn't know falls back
/// to the default sort.
pub(crate) fn from_state(state: &Value) -> Option<SavedEvents> {
    let sort = match &state["sort"] {
        Value::Null => None,
        sort => EventColumn::from_id(sort["column"].as_str()?)
            .zip(sort_from_name(sort["order"].as_str()?)),
    };
    Some(SavedEvents {
        context_name: state["context_name"].as_str()?.to_string(),
        namespaces: serde_json::from_value(state["namespaces"].clone()).unwrap_or_default(),
        sort: sort.or(Some(super::table::DEFAULT_SORT)),
        filters: serde_json::from_value(state["filters"].clone()).unwrap_or_default(),
    })
}

/// Rebuilds the browser `state` describes, with its saved namespaces and sort.
pub(crate) fn restore(state: &Value, cx: &mut App) -> Arc<dyn PanelView> {
    let saved = from_state(state).expect("an events browser's layout state must name its cluster");
    let scope = PanelScope::new(
        NavTarget::Kind(DiscoveredKind::events()),
        saved.context_name.clone(),
    )
    .scoped_to(saved.namespaces.clone());
    panel_handle(cx.new(|cx| {
        let mut panel = EventsPanel::new(scope, cx);
        panel.initial_sort = saved.sort;
        panel.filters = saved.filters;
        panel
    }))
}
