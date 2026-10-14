//! Open in New Panel for the focused panel (`open-panel-new-instance` D4):
//! another instance of the panel with focus - the same resource, context and
//! namespaces - opened past the window's one-panel-per-resource rule.

use super::*;
use crate::ui::nav::{OpenInNewPanel, OpenMode};

impl MainWindow {
    /// `OpenInNewPanel`: opens the focused dock panel's key again as a new
    /// instance, focused. A typed label Logs panel opens a new empty typed
    /// panel instead, as Open Logs by Label does (D6). Nothing with focus
    /// outside the dock.
    pub(super) fn on_action_open_in_new_panel(
        &mut self,
        _: &OpenInNewPanel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace {
            dock_area,
            open_panels,
            ..
        } = &self.mode
        else {
            return;
        };
        let focused = {
            let area = dock_area.read(cx);
            crate::ui::panel::focus::focused_group(area, window, cx)
                .and_then(|node| crate::ui::panel::tabs::active_panel_of(area, node))
        };
        let Some(key) = focused.and_then(|id| {
            open_panels
                .iter()
                .find(|open| open.id == id)
                .map(|open| open.key.clone())
        }) else {
            return;
        };
        let (target, mode) = match &key.target {
            NavTarget::LabelLogs(source) if source.is_typed() => (
                NavTarget::LabelLogs(crate::util::logs::LabelLogs::new_typed()),
                OpenMode::Foreground,
            ),
            target => (target.clone(), OpenMode::NewPanel),
        };
        self.open_target_in(
            target,
            None,
            Some(key.context_name),
            key.namespaces,
            mode,
            window,
            cx,
        );
    }
}

#[cfg(test)]
#[path = "new_instance_tests.rs"]
mod tests;
