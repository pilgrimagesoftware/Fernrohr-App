//! Saving and restoring a Helm Releases list with the window's dock layout:
//! its context, namespaces, sort, search text and whether uninstalled
//! releases show.

use super::columns::HelmColumn;
use super::panel::HelmReleasesPanel;
use crate::ui::list_search::ListSearch;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::base::dock::PanelView;
use gpui_kit::component::dock::{panel_handle, register_panel};
use gpui_kit::component::table::ColumnSort;
use gpui_kit::*;
use serde_json::{Value, json};
use std::sync::Arc;

/// Registers the dock's restore for saved Helm Releases lists.
pub fn register_restore(cx: &mut App) {
    register_panel(cx, "HelmReleases", |context, _window, cx| {
        crate::ui::unrestored::restore_with(&context, cx, restore)
    });
}

/// One saved list, as [`dump`] wrote it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SavedHelmReleases {
    pub(crate) context_name: String,
    pub(crate) namespaces: Vec<String>,
    pub(super) sort: Option<(HelmColumn, ColumnSort)>,
    pub(super) filter: Option<String>,
    pub(super) show_uninstalled: bool,
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
pub(super) fn dump(panel: &HelmReleasesPanel, cx: &App) -> Value {
    let sort = panel
        .sort(cx)
        .map(|(column, sort)| json!({ "column": column.id(), "order": sort_name(sort) }));
    crate::ui::panel_title::with_saved_scope(
        &panel.scope,
        json!({
            "context_name": panel.scope.context_name,
            "namespaces": panel.scope.namespaces,
            "sort": sort,
            "filter": panel.search.dump(cx),
            "show_uninstalled": panel.show_uninstalled,
        }),
    )
}

/// The list a saved panel was, or `None` for state that doesn't name its
/// cluster. An unknown sort column or order counts as no sort of its own.
pub(crate) fn from_state(state: &Value) -> Option<SavedHelmReleases> {
    let sort = match &state["sort"] {
        Value::Null => None,
        sort => HelmColumn::from_id(sort["column"].as_str()?)
            .zip(sort_from_name(sort["order"].as_str()?)),
    };
    Some(SavedHelmReleases {
        context_name: state["context_name"].as_str()?.to_string(),
        namespaces: serde_json::from_value(state["namespaces"].clone()).unwrap_or_default(),
        sort,
        filter: state["filter"].as_str().map(str::to_string),
        show_uninstalled: state["show_uninstalled"].as_bool().unwrap_or(false),
    })
}

/// Rebuilds the list `state` describes. `Err` for state that doesn't name its
/// cluster.
pub(crate) fn restore(state: &Value, cx: &mut App) -> Result<Arc<dyn PanelView>, String> {
    let saved = from_state(state).ok_or("its state doesn't name its cluster")?;
    let scope = PanelScope::new(NavTarget::HelmReleases, saved.context_name.clone())
        .scoped_to(saved.namespaces.clone())
        .restored_from(state);
    Ok(panel_handle(cx.new(|cx| {
        let mut panel = HelmReleasesPanel::new(scope, cx);
        panel.initial_sort = saved.sort;
        panel.search = ListSearch::restored(saved.filter);
        panel.show_uninstalled = saved.show_uninstalled;
        panel
    })))
}
