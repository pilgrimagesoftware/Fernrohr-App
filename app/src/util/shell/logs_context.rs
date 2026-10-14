//! Switching a typed label Logs panel to another of the window's contexts
//! (`selector-logs-context-picker`): the panel asks by dispatching
//! [`SwitchLabelLogsContext`], and the window - which owns the dock - answers
//! here.

use super::*;
use crate::util::logs::{LogsPanel, SwitchLabelLogsContext};
use gpui_kit::component::dock::InsertTarget;

impl MainWindow {
    /// `SwitchLabelLogsContext`: the panel it names, if it's this window's
    /// Logs panel, moves to the context it names.
    pub(super) fn on_action_switch_label_logs_context(
        &mut self,
        action: &SwitchLabelLogsContext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace { dock_area, .. } = &self.mode else {
            return;
        };
        let id = gpui_kit::component::dock::PanelId::from(action.panel);
        let Some(nav::OpenedPanel::Logs(from)) = nav::opened_panel_for(dock_area.read(cx), id, cx)
        else {
            return;
        };
        self.switch_label_logs_context(from, action.context.clone(), window, cx);
    }

    /// Moves the typed panel `from` to `context`, whatever other typed panels
    /// already search it (`selector-logs-multiple-panels` D4): the same
    /// instance opens in `context` in `from`'s tab, with its selector and
    /// container filter over all namespaces, and `from` closes (D1) - which
    /// ends its streams and its hold on the old context's Pods watch, as any
    /// close does.
    pub(super) fn switch_label_logs_context(
        &mut self,
        from: Entity<LogsPanel>,
        context: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace {
            contexts,
            dock_area,
            ..
        } = &self.mode
        else {
            return;
        };
        if !contexts.contains(&context) {
            return;
        }
        let dock_area = dock_area.clone();
        // The same instance, in the new context: the context is part of its
        // key, so it can't collide with a typed panel already searching there.
        let target = from.read(cx).label_target();
        if !matches!(&target, NavTarget::LabelLogs(source) if source.is_typed()) {
            return;
        }
        let from_id = nav::OpenedPanel::Logs(from.clone()).panel_id();
        let slot = {
            let area = dock_area.read(cx);
            crate::ui::panel::tabs::group_of(area, from_id).and_then(|node| {
                let tabs = crate::ui::panel::tabs::tabs_of(area, node)?;
                Some((node, tabs.panels.iter().position(|id| *id == from_id)?))
            })
        };
        let (selector, container) = from.read(cx).carried();
        let Some(shown) = self.open_target_in(
            target,
            None,
            Some(context),
            Vec::new(),
            crate::ui::nav::OpenMode::Foreground,
            window,
            cx,
        ) else {
            return;
        };
        let WindowMode::Workspace { open_panels, .. } = &mut self.mode else {
            return;
        };
        let replacement = open_panels
            .iter()
            .find(|open| open.id == shown.id)
            .and_then(|open| match &open.panel {
                Some(nav::OpenedPanel::Logs(panel)) => Some(panel.clone()),
                _ => None,
            });
        open_panels.retain(|open| open.id != from_id);
        if let Some(replacement) = &replacement {
            replacement.update(cx, |panel, cx| panel.carry_over(selector, container, cx));
        }
        dock_area.update(cx, |area, cx| {
            if let Some((node, ix)) = slot {
                let target = InsertTarget::Tabs {
                    node,
                    ix: Some(ix),
                    activate: true,
                };
                area.move_panel(shown.id, target, window, cx);
            }
            area.remove_panel(from, window, cx);
        });
        if let Some(panel) = dock_area.read(cx).panel(shown.id) {
            window.focus(&panel.focus_handle(cx), cx);
        }
        cx.notify();
    }
}
