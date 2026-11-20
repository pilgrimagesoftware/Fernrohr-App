//! View Logs (`l`): the pod's logs from its detail panel, through the same
//! `SelectedPod` + `ShowLogs` path the Pods list's `l` takes - so the Logs
//! panel opens (or focuses) on this pod, its first container selected.

use super::commands::{ViewLogs, ViewLogsFlipped};
use super::fetch::PodDetailState;
use super::panel::PodDetailPanel;
use crate::k8s::resource::pods::{PodSelection, SelectedPod, long_running_names};
use gpui_kit::*;

impl PodDetailPanel {
    /// The selection the Pods list would publish for this pod: its app
    /// containers in spec order, then its sidecars. `None` until the pod has
    /// loaded - there is no container list to hand Logs before then.
    pub(super) fn log_selection(&self) -> Option<PodSelection> {
        let PodDetailState::Loaded(pod) = &self.state else {
            return None;
        };
        let (containers, sidecars) = long_running_names(pod);
        Some(PodSelection {
            namespace: self.pod.namespace.clone(),
            name: self.pod.name.clone(),
            containers,
            sidecars,
            context_name: self.scope.context_name.clone(),
        })
    }

    pub(super) fn on_action_view_logs(
        &mut self,
        _: &ViewLogs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.log_selection() else {
            return;
        };
        cx.set_global(SelectedPod(Some(selection)));
        window.dispatch_action(Box::new(crate::ui::nav::ShowLogs), cx);
    }

    /// `ViewLogsFlipped` (`shift-l`): this pod's logs the other way from the
    /// Logs panels preference - its own panel, or the shared one.
    pub(super) fn on_action_view_logs_flipped(
        &mut self,
        _: &ViewLogsFlipped,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.log_selection() else {
            return;
        };
        cx.set_global(SelectedPod(Some(selection)));
        window.dispatch_action(Box::new(crate::ui::nav::ShowLogsFlipped), cx);
    }
}
