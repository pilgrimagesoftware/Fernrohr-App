//! The sort the user last chose for each kind of list (`remembered-list-sort`):
//! what a header click or a keyboard sort command records, and what a list
//! opened afterwards starts with, whatever its context or window. App-wide,
//! one sort per kind, and saved with the workspace
//! (`WorkspaceConfig::sort_defaults`) - like [`super::namespace_defaults`],
//! whose shape this follows.

use crate::config::workspace::SortState;
use gpui_kit::{App, Global};
use std::collections::BTreeMap;

/// Kind key (`ui::list_sort::kind_key`) to its remembered sort.
#[derive(Default)]
pub(crate) struct SortDefaults(BTreeMap<String, SortState>);

impl Global for SortDefaults {}

impl SortDefaults {
    /// Starts the app with the sorts saved last time.
    pub(super) fn load(cx: &mut App, defaults: BTreeMap<String, SortState>) {
        cx.set_global(Self(defaults));
    }

    /// Every remembered sort, for saving the workspace.
    pub(super) fn snapshot(cx: &App) -> BTreeMap<String, SortState> {
        cx.try_global::<Self>()
            .map(|defaults| defaults.0.clone())
            .unwrap_or_default()
    }

    /// The sort remembered for `key`, if the user ever chose one.
    pub(crate) fn get(cx: &App, key: &str) -> Option<SortState> {
        cx.try_global::<Self>()?.0.get(key).cloned()
    }

    /// Remembers `sort` for `key` and saves the workspace soon after - the
    /// debounced save, so a run of header clicks writes once.
    pub(crate) fn record(cx: &mut App, key: &str, sort: SortState) {
        cx.default_global::<Self>().0.insert(key.to_string(), sort);
        super::persist::schedule_save(cx);
    }
}

#[cfg(test)]
mod tests;
