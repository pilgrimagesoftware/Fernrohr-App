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
    /// The containers a picker offers: app containers, then sidecars (#398).
    pub containers: Vec<String>,
    /// Which of `containers` are sidecars, so a picker can mark them.
    pub sidecars: Vec<String>,
    pub context_name: String,
}

impl PodSelection {
    /// `pod`, addressed in `context_name`, with the containers a picker
    /// offers - its app containers, then its sidecars.
    pub(crate) fn of(pod: &Pod, context_name: &str) -> Self {
        let (containers, sidecars) = super::sidecars::long_running_names(pod);
        Self {
            namespace: pod.metadata.namespace.clone().unwrap_or_default(),
            name: pod.metadata.name.clone().unwrap_or_default(),
            containers,
            sidecars,
            context_name: context_name.to_string(),
        }
    }

    /// How a picker names `container`: marked when it is a sidecar.
    pub(crate) fn container_label(&self, container: &str) -> String {
        super::sidecars::container_label(container, self.sidecars.iter().any(|s| s == container))
    }

    /// This pod's detail panel, opened as a new instance (`open-panel-new-instance`).
    pub(crate) fn new_panel_open(&self) -> crate::ui::nav::OpenPodInNewPanel {
        crate::ui::nav::OpenPodInNewPanel {
            context_name: self.context_name.clone(),
            namespace: self.namespace.clone(),
            name: self.name.clone(),
        }
    }

    /// This pod's detail panel, opened in the background.
    pub(crate) fn background_open(&self) -> crate::ui::nav::OpenPodInBackground {
        crate::ui::nav::OpenPodInBackground {
            context_name: self.context_name.clone(),
            namespace: self.namespace.clone(),
            name: self.name.clone(),
        }
    }
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
