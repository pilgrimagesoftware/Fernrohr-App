//! Keeping keyboard focus on a panel when the dock changes under it
//! (`panel-tab-focus-on-close`): closing the focused tab leaves focus on nothing,
//! since the dock removes the panel without moving focus anywhere. Opening and
//! switching tabs focus explicitly where they're handled (`open`, `tabs`). It also
//! notes the dock panel that last held focus, for where a new panel opens.

use super::{MainWindow, OpenPanel, WindowMode};
use crate::ui::panel::{focus, tabs};
use gpui_kit::component::dock::{DockArea, PanelId};
use gpui_kit::{App, Context, Entity, Subscription, Window};

impl MainWindow {
    /// Watches dock panel `id` for focus, noting it as the window's last-focused
    /// dock panel each time focus enters it - the group a new panel joins while
    /// focus is outside the dock (`open_target_in`). `None` for an id the dock
    /// doesn't hold.
    pub(super) fn watch_panel_focus(
        dock_area: &Entity<DockArea>,
        id: PanelId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Subscription> {
        let handle = dock_area.read(cx).panel(id)?.focus_handle(cx);
        Some(cx.on_focus_in(&handle, window, move |this, _window, _cx| {
            if let WindowMode::Workspace {
                last_focused_panel, ..
            } = &mut this.mode
            {
                *last_focused_panel = Some(id);
            }
        }))
    }

    /// Focuses the dock's first displayed panel - a window entering its workspace,
    /// new or restored, starts with a panel's keys live rather than focus on no
    /// panel. With no panel open, the Resource panel, else the window.
    pub(super) fn focus_displayed_panel(&self, window: &mut Window, cx: &mut App) {
        let target = match &self.mode {
            WindowMode::Workspace {
                dock_area,
                resource_panel,
                resource_collapsed,
                ..
            } => {
                let area = dock_area.read(cx);
                focus::dock_stops(area, cx)
                    .first()
                    .and_then(|&panel| area.panel(panel))
                    .map(|view| view.focus_handle(cx))
                    .or_else(|| {
                        (!*resource_collapsed).then(|| resource_panel.read(cx).focus_handle())
                    })
            }
            WindowMode::Picker(_) => None,
        };
        window.focus(&target.unwrap_or_else(|| self.focus_handle.clone()), cx);
    }

    /// Run on every dock layout change, before closed panels are forgotten. If
    /// the change closed the panel that held focus, focus moves to the tab now
    /// displayed in its group; with the group gone, to the first open panel; with
    /// none, to the Resource panel or the window. Then every open panel's group is
    /// noted, for the next close to read.
    pub(super) fn keep_focus_on_a_panel(
        &mut self,
        dock_area: &Entity<DockArea>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace {
            open_panels,
            resource_panel,
            resource_collapsed,
            ..
        } = &mut self.mode
        else {
            return;
        };
        let area = dock_area.read(cx);
        let closed: Vec<&OpenPanel> = open_panels
            .iter()
            .filter(|open| area.panel(open.id).is_none())
            .collect();
        // Lost when focus is in this window's view but no longer in a panel that's
        // still open: on the closed panel's handle, or on the window root. Asked of
        // the panels that remain, not of the closed one - a restored panel has no
        // handle here to ask, and its own lingers as focused until the next frame,
        // with no element left to dispatch keys from. Focus outside the view (a
        // dialog over it) isn't the dock's to move.
        let still_held = focus::dock_stops(area, cx)
            .into_iter()
            .filter_map(|panel| area.panel(panel))
            .any(|view| view.focus_handle(cx).contains_focused(window, cx))
            || (!*resource_collapsed
                && resource_panel
                    .read(cx)
                    .focus_handle()
                    .contains_focused(window, cx));
        let lost = !closed.is_empty()
            && !still_held
            && (window.focused(cx).is_none() || self.focus_handle.contains_focused(window, cx));
        if lost {
            let target = closed
                .iter()
                .find_map(|open| open.group)
                .and_then(|node| tabs::active_panel_of(area, node))
                .or_else(|| focus::dock_stops(area, cx).first().copied())
                .and_then(|panel| area.panel(panel))
                .map(|view| view.focus_handle(cx))
                .or_else(|| (!*resource_collapsed).then(|| resource_panel.read(cx).focus_handle()))
                .unwrap_or_else(|| self.focus_handle.clone());
            window.focus(&target, cx);
        }
        let area = dock_area.read(cx);
        for open in open_panels.iter_mut() {
            if let Some(node) = tabs::group_of(area, open.id) {
                open.group = Some(node);
            }
        }
    }
}

#[cfg(test)]
mod close_tests;
#[cfg(test)]
mod open_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod traversal_tests;
