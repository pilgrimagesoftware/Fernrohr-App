//! What a panel's saved state keeps of its [`PanelScope`] beyond the panel's
//! own fields: its instance number and whether it is pinned.

use super::PanelScope;

/// The instance number a panel's saved state holds (`open-panel-new-instance`
/// D5), 0 when it holds none - every layout saved before instances existed.
pub fn saved_instance(state: &serde_json::Value) -> u32 {
    state["instance"]
        .as_u64()
        .and_then(|instance| u32::try_from(instance).ok())
        .unwrap_or(0)
}

/// Whether a panel's saved state says it's pinned - `false` when it says
/// nothing, as every layout from before pinning does.
pub fn saved_pinned(state: &serde_json::Value) -> bool {
    state["pinned"].as_bool().unwrap_or(false)
}

/// A panel's saved `state` with what `scope` keeps beyond the panel's own
/// fields: its instance number unless it's the first, and `pinned` when it's
/// pinned - so a first, unpinned panel saves exactly as it did before.
pub fn with_saved_scope(scope: &PanelScope, mut state: serde_json::Value) -> serde_json::Value {
    if scope.instance != 0 {
        state["instance"] = scope.instance.into();
    }
    if scope.pinned {
        state["pinned"] = true.into();
    }
    state
}
