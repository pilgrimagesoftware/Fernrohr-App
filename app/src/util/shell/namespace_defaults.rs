//! Each context's default namespace scope (`warp-all-to-namespace`): what Warp All
//! to Namespace sets, and what a namespaced list opened afterwards in that context
//! starts scoped to. A list of namespaces, so a set can be a default too. App-wide - one default per context, whichever window opens
//! the panel - and saved with the workspace (`WorkspaceConfig::namespace_defaults`).

use gpui_kit::{App, Global};
use std::collections::BTreeMap;

/// Context name to its default namespace scope.
#[derive(Default)]
pub(super) struct NamespaceDefaults(BTreeMap<String, Vec<String>>);

impl Global for NamespaceDefaults {}

impl NamespaceDefaults {
    /// Replaces every default with `defaults` - what the saved workspace held.
    pub(super) fn load(cx: &mut App, defaults: BTreeMap<String, Vec<String>>) {
        cx.set_global(Self(defaults));
    }

    /// Every default, for saving.
    pub(super) fn snapshot(cx: &App) -> BTreeMap<String, Vec<String>> {
        cx.try_global::<Self>()
            .map(|defaults| defaults.0.clone())
            .unwrap_or_default()
    }

    /// `context_name`'s default namespace scope, if one was set.
    pub(super) fn get(cx: &App, context_name: &str) -> Option<Vec<String>> {
        cx.try_global::<Self>()?.0.get(context_name).cloned()
    }

    /// Makes `namespaces` `context_name`'s default, and schedules a save.
    pub(super) fn set(cx: &mut App, context_name: &str, namespaces: Vec<String>) {
        cx.default_global::<Self>()
            .0
            .insert(context_name.to_string(), namespaces);
        super::persist::schedule_save(cx);
    }
}
