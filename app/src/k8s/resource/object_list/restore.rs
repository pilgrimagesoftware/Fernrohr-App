//! Saving and restoring a list panel with the window's dock layout (`standard-
//! resource-panels` 1.6): its kind, context, namespace selection and column layout.
//! One pair of functions owns the saved shape - [`dump`] and [`from_state`] - read
//! by the dock's restore and by the window when it rebuilds its panel keys.

use super::panel::ObjectListPanel;
use super::table::ColumnLayout;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::discovery_registry::DiscoveryRegistry;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use crate::ui::placeholder::PlaceholderPanel;
use gpui_kit::base::dock::PanelView;
use gpui_kit::component::dock::{panel_handle, register_panel};
use gpui_kit::*;
use kube::core::GroupVersionKind;
use serde_json::{Value, json};
use std::sync::Arc;

/// Registers the dock's restore for saved list panels.
pub fn register_restore(cx: &mut App) {
    register_panel(cx, "ObjectList", |context, _window, cx| {
        crate::ui::unrestored::restore_with(&context, cx, restore)
    });
}

/// One saved list panel, as [`dump`] wrote it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SavedList {
    pub(crate) kind: DiscoveredKind,
    pub(crate) context_name: String,
    pub(crate) namespaces: Vec<String>,
    pub(super) columns: ColumnLayout,
    /// The filter text last saved with this panel, if any
    /// (`saved-panel-layouts` 1.6).
    pub(super) filter: Option<String>,
    /// The sort column id and direction last saved with this panel, if any
    /// (`saved-panel-layouts` 1.6).
    pub(super) sort: Option<(String, bool)>,
}

/// What `panel` saves.
pub(super) fn dump(panel: &ObjectListPanel, cx: &App) -> Value {
    let gvk = &panel.kind.gvk;
    let columns = match &panel.table {
        Some(table) => table.read(cx).delegate().layout(),
        // Never drawn, so the layout it was given is still the one it has.
        None => panel.initial_layout.clone(),
    };
    let filter = match &panel.filter {
        Some(input) => Some(input.read(cx).value().to_string()),
        None => panel.initial_filter.clone(),
    };
    let sort = match &panel.table {
        Some(table) => table
            .read(cx)
            .delegate()
            .sort_state()
            .map(|(column, descending)| (column.to_string(), descending)),
        None => panel.initial_sort.clone(),
    };
    let sort =
        sort.map(|(column, descending)| json!({ "column": column, "descending": descending }));
    json!({
        "context_name": panel.scope.context_name,
        "namespaces": panel.scope.namespaces,
        "group": gvk.group,
        "version": gvk.version,
        "kind": gvk.kind,
        "plural": panel.kind.plural,
        "namespaced": panel.kind.namespaced,
        "columns": columns,
        "filter": filter,
        "sort": sort,
    })
}

/// The list a saved panel was, or `None` for state that doesn't name its kind and
/// cluster. Also reads a pre-list `Resource` placeholder's state, which saved the
/// same kind fields and no columns.
pub(crate) fn from_state(state: &Value) -> Option<SavedList> {
    Some(SavedList {
        kind: DiscoveredKind {
            gvk: GroupVersionKind::gvk(
                state["group"].as_str()?,
                state["version"].as_str()?,
                state["kind"].as_str()?,
            ),
            plural: state["plural"].as_str()?.to_string(),
            namespaced: state["namespaced"].as_bool()?,
            verbs: Default::default(),
        },
        context_name: state["context_name"].as_str()?.to_string(),
        namespaces: serde_json::from_value(state["namespaces"].clone()).unwrap_or_default(),
        columns: serde_json::from_value(state["columns"].clone()).unwrap_or_default(),
        filter: state["filter"].as_str().map(str::to_string),
        sort: sort_from_state(state),
    })
}

/// The `sort` a saved list panel names, if any (`saved-panel-layouts` 1.6):
/// `{ "column": <id>, "descending": <bool> }`, or absent/`null` for no saved
/// sort - true of every layout saved before this field existed.
fn sort_from_state(state: &Value) -> Option<(String, bool)> {
    let sort = &state["sort"];
    let column = sort["column"].as_str()?.to_string();
    let descending = sort["descending"].as_bool().unwrap_or(false);
    Some((column, descending))
}

/// `standard-resource-panels` D4: a saved list comes back as a list - unless its
/// cluster's discovery has finished and no longer reports the kind, when the
/// placeholder stands in for it rather than a list that can never fill. Discovery
/// still running (the usual case at startup) counts as served.
pub(crate) fn restores_as_placeholder(
    kind: &DiscoveredKind,
    discovered: Option<&[DiscoveredKind]>,
) -> bool {
    discovered.is_some_and(|kinds| !kinds.contains(kind))
}

/// Rebuilds the panel `state` describes: a list panel with its saved namespaces and
/// column layout, or - for a kind its cluster no longer serves - the placeholder.
/// `Err` for state that doesn't name its kind and cluster.
pub(crate) fn restore(state: &Value, cx: &mut App) -> Result<Arc<dyn PanelView>, String> {
    let saved = from_state(state).ok_or("its state doesn't name its kind and cluster")?;
    let scope = PanelScope::new(
        NavTarget::Kind(saved.kind.clone()),
        saved.context_name.clone(),
    )
    .scoped_to(saved.namespaces.clone());
    let discovery = DiscoveryRegistry::kinds(cx, &saved.context_name);
    if restores_as_placeholder(&saved.kind, discovery.read(cx).kinds()) {
        return Ok(panel_handle(
            cx.new(|cx| PlaceholderPanel::new(saved.kind, scope, cx)),
        ));
    }
    Ok(panel_handle(cx.new(|cx| {
        let mut panel = ObjectListPanel::new(saved.kind, scope, cx);
        panel.initial_layout = saved.columns;
        panel.initial_filter = saved.filter;
        panel.initial_sort = saved.sort;
        panel
    })))
}

#[cfg(test)]
mod tests;
