//! Each context's Resource panel groups as the user last left them (#357):
//! what collapsing or expanding a section or subgroup records, and what a
//! panel opened on, or switched to, that context starts with - whatever its
//! window. Saved with the workspace (`WorkspaceConfig::resource_groups`), in
//! the shape of [`super::sort_defaults`].

use crate::config::workspace::ResourceGroups;
use gpui_kit::{App, Global};
use std::collections::BTreeMap;

/// Context name to its remembered groups.
#[derive(Default)]
pub(crate) struct ResourceGroupMemory(BTreeMap<String, ResourceGroups>);

impl Global for ResourceGroupMemory {}

impl ResourceGroupMemory {
    /// Starts the app with the groups saved last time.
    pub(super) fn load(cx: &mut App, groups: BTreeMap<String, ResourceGroups>) {
        cx.set_global(Self(groups));
    }

    /// Every context's remembered groups, for saving the workspace.
    pub(super) fn snapshot(cx: &App) -> BTreeMap<String, ResourceGroups> {
        cx.try_global::<Self>()
            .map(|memory| memory.0.clone())
            .unwrap_or_default()
    }

    /// The groups remembered for `context`, if the user ever changed them.
    pub(crate) fn get(cx: &App, context: &str) -> Option<ResourceGroups> {
        cx.try_global::<Self>()?.0.get(context).cloned()
    }

    /// Remembers `groups` for `context` and saves the workspace soon after -
    /// the debounced save, so folding several groups in a row writes once.
    /// Back at the default, the context is forgotten rather than stored empty.
    pub(crate) fn record(cx: &mut App, context: &str, groups: ResourceGroups) {
        let memory = &mut cx.default_global::<Self>().0;
        if groups == ResourceGroups::default() {
            memory.remove(context);
        } else {
            memory.insert(context.to_string(), groups);
        }
        super::persist::schedule_save(cx);
    }
}
