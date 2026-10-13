//! Opening a pod's logs (`logs-panel-instancing`): in a Logs panel of the
//! pod's own, or in the one Logs panel that follows the selection - per the
//! Logs panels preference (`ui::logs_panels`), or the other way for one open
//! with the flipped action.
//!
//! Every way of opening logs - `l` in the Pods list or a pod's detail, the
//! Pods table's menu, the palette, `cmd-2` - publishes the pod as
//! [`SelectedPod`] and dispatches [`ShowLogs`] or [`ShowLogsFlipped`], so the
//! choice is made here, once.

use super::*;
use crate::config::ui::LogsPanels;
use crate::ui::nav::{PodRef, ShowLogsFlipped};

impl MainWindow {
    /// `ShowLogs`: the selected pod's logs, where the preference says.
    pub(super) fn on_action_show_logs(
        &mut self,
        _: &ShowLogs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panels = crate::ui::logs_panels::current(cx);
        self.show_logs(panels, window, cx);
    }

    /// `ShowLogsFlipped`: the selected pod's logs, the other way from the
    /// preference, this once.
    pub(super) fn on_action_show_logs_flipped(
        &mut self,
        _: &ShowLogsFlipped,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panels = crate::ui::logs_panels::current(cx).flipped();
        self.show_logs(panels, window, cx);
    }

    /// Opens the selected pod's logs `panels`' way. Per pod, that pod's own
    /// panel - focused if it's open, and switched to the selection's container
    /// if that differs; with nothing selected, or reusing, the one Logs panel.
    fn show_logs(&mut self, panels: LogsPanels, window: &mut Window, cx: &mut Context<Self>) {
        let selection = cx
            .try_global::<SelectedPod>()
            .and_then(|selected| selected.0.clone());
        let (LogsPanels::PerPod, Some(selection)) = (panels, selection) else {
            self.open_target(NavTarget::Logs, window, cx);
            return;
        };
        let target = NavTarget::PodLogs(PodRef {
            namespace: selection.namespace.clone(),
            name: selection.name.clone(),
        });
        // The pod's panel if it's already open here. One built by the open
        // below reads the selection itself; only an existing one is told it,
        // in case it names another container.
        let existing = match &self.mode {
            WindowMode::Workspace { open_panels, .. } => open_panels
                .iter()
                .find(|open| {
                    open.key.target == target && open.key.context_name == selection.context_name
                })
                .and_then(|open| match &open.panel {
                    Some(crate::ui::nav::OpenedPanel::Logs(panel)) => Some(panel.clone()),
                    _ => None,
                }),
            WindowMode::Picker(_) => None,
        };
        self.open_target(target, window, cx);
        if let Some(panel) = existing {
            panel.update(cx, |panel, cx| panel.pin_to(selection, cx));
        }
    }
}

impl MainWindow {
    /// `ShowLabelLogs` (#150): the Logs panel a label selector is typed into,
    /// opened - or, already open, focused with its field focused to type
    /// another.
    pub(super) fn on_action_show_label_logs(
        &mut self,
        _: &crate::util::logs::ShowLabelLogs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = NavTarget::LabelLogs(crate::util::logs::LabelLogs::Typed);
        // In the focused list's context and namespaces (`selector-logs`): in a
        // window holding several contexts, the active one may not be the
        // list's, and its cluster hasn't the pods the list shows (#213). The
        // open panel keeps its own scope - it's found by it.
        let focused = self.focused_namespaced(window, cx).map(|(_, key)| key);
        let context_name = focused.as_ref().map(|key| key.context_name.clone());
        let (existing, namespaces) =
            match self.open_logs_panel(&target, context_name.as_deref(), cx) {
                Some((panel, namespaces)) => (Some(panel), namespaces),
                None => (None, focused.map(|key| key.namespaces).unwrap_or_default()),
            };
        self.open_target_in(
            target,
            None,
            context_name,
            namespaces,
            crate::ui::nav::OpenMode::Foreground,
            window,
            cx,
        );
        if let Some(panel) = existing {
            panel.update(cx, |panel, cx| panel.focus_selector(window, cx));
        }
    }

    /// `OpenWorkloadLogs` (#150): the Logs panel following a workload's pods,
    /// in the workload's own context.
    pub(super) fn on_action_open_workload_logs(
        &mut self,
        action: &crate::util::logs::OpenWorkloadLogs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_target_in(
            NavTarget::LabelLogs(crate::util::logs::LabelLogs::Workload(
                action.workload.clone(),
            )),
            None,
            Some(action.context_name.clone()),
            Vec::new(),
            crate::ui::nav::OpenMode::Foreground,
            window,
            cx,
        );
    }

    /// The Logs panel this window has open on `target`, in `context_name` -
    /// else the context a new one would open in - with the namespaces it's
    /// scoped to.
    fn open_logs_panel(
        &self,
        target: &NavTarget,
        context_name: Option<&str>,
        cx: &App,
    ) -> Option<(Entity<crate::util::logs::LogsPanel>, Vec<String>)> {
        let WindowMode::Workspace {
            open_panels,
            contexts,
            active,
            ..
        } = &self.mode
        else {
            return None;
        };
        let context_name = match context_name {
            Some(named) => named.to_string(),
            None => super::panels::pod_scoped_context(target, contexts, *active, cx)?,
        };
        open_panels
            .iter()
            .find(|open| &open.key.target == target && open.key.context_name == context_name)
            .and_then(|open| match &open.panel {
                Some(crate::ui::nav::OpenedPanel::Logs(panel)) => {
                    Some((panel.clone(), open.key.namespaces.clone()))
                }
                _ => None,
            })
    }
}

#[cfg(test)]
mod tests;
