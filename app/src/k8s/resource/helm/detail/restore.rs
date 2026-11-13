//! Saving and restoring a release detail panel with the window's layout: its
//! context, the release, and its tab.

use super::panel::{HelmReleasePanel, ReleaseTab};
use crate::k8s::resource::helm::ReleaseKey;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::base::dock::PanelView;
use gpui_kit::component::dock::{panel_handle, register_panel};
use gpui_kit::*;
use serde_json::{Value, json};
use std::sync::Arc;

/// Registers the dock's restore for saved release detail panels.
pub fn register_restore(cx: &mut App) {
    register_panel(cx, "HelmRelease", |context, _window, cx| {
        crate::ui::unrestored::restore_with(&context, cx, restore)
    });
}

/// What `panel` saves.
pub(super) fn dump(panel: &HelmReleasePanel) -> Value {
    crate::ui::panel_title::with_saved_scope(
        &panel.scope,
        json!({
            "context_name": panel.scope.context_name,
            "release_namespace": panel.key.namespace,
            "release_name": panel.key.name,
            "tab": panel.tab.id(),
        }),
    )
}

/// The context and release a saved panel showed, or `None` for state that
/// doesn't name them.
pub(crate) fn from_state(state: &Value) -> Option<(String, ReleaseKey)> {
    Some((
        state["context_name"].as_str()?.to_string(),
        ReleaseKey {
            namespace: state["release_namespace"].as_str()?.to_string(),
            name: state["release_name"].as_str()?.to_string(),
        },
    ))
}

/// Rebuilds the panel `state` describes, on its saved tab.
pub(crate) fn restore(state: &Value, cx: &mut App) -> Result<Arc<dyn PanelView>, String> {
    let (context_name, key) = from_state(state).ok_or("its state doesn't name its release")?;
    let tab = state["tab"]
        .as_str()
        .and_then(ReleaseTab::from_id)
        .unwrap_or_default();
    let scope =
        PanelScope::new(NavTarget::HelmRelease(key.clone()), context_name).restored_from(state);
    Ok(panel_handle(cx.new(|cx| {
        let mut panel = HelmReleasePanel::new(key, scope, cx);
        panel.tab = tab;
        panel
    })))
}
