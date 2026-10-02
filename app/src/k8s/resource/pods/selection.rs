//! The selected pod, shared app-wide so the window can open its logs or detail.

use super::*;

/// The pod a Logs panel should stream, set by clicking a row in a Pods
/// panel. App-scoped rather than a direct link between the two panels, since
/// either can live in any dock split of any window.
///
/// `context_name` is the cluster context of the Pods panel that published this
/// selection - not necessarily the window's active context in a multi-context
/// window. Without it, a pod selected from one context's Pods panel would open
/// Logs or a detail panel against whichever context happened to be active
/// (`1-window-context-bar` bug 1): `pods "..." not found` against a cluster
/// the pod was never in.
#[derive(Debug, Clone, PartialEq)]
pub struct PodSelection {
    pub namespace: String,
    pub name: String,
    pub containers: Vec<String>,
    pub context_name: String,
}

#[derive(Default)]
pub struct SelectedPod(pub Option<PodSelection>);

impl Global for SelectedPod {}

/// Tells a Pods table which pod its own row click picked, so a later sort or
/// row update can move the highlight with that pod (`pods_table::reselect`).
pub(super) fn remember_selection<V>(
    table: &Entity<TableState<PodTableDelegate>>,
    selection: &PodSelection,
    cx: &mut Context<V>,
) {
    let selection = selection.clone();
    table.update(cx, |table, _| {
        table.delegate_mut().remember_selection(Some(selection));
    });
}
