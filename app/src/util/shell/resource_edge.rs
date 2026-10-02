//! `MainWindow`'s handlers for moving the Resource panel to the other window edge,
//! saving that edge as the default, and collapsing it (`cluster-picker-and-navigation`
//! 11.1-11.3). The commands and buttons are `ui::panel::resource`'s; the state lives
//! in `WindowMode`.

use super::{MainWindow, WindowMode};
#[cfg(test)]
use crate::ui::resource_panel::ResourceSide;
use crate::ui::resource_panel::{MoveResourcePanel, SaveResourceSide, ToggleResourcePanel};
use gpui_kit::{Context, Window};

impl MainWindow {
    /// Collapses or expands the Resource panel. Collapsing while it holds focus
    /// hands focus to the dock's displayed panel (or the window), so the keyboard
    /// isn't left on a panel that isn't drawn; expanding focuses the panel, since
    /// whoever asked for it back wants to use it.
    pub(super) fn on_action_toggle_resource_panel(
        &mut self,
        _: &ToggleResourcePanel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace {
            resource_panel,
            resource_collapsed,
            dock_area,
            ..
        } = &mut self.mode
        else {
            return;
        };
        *resource_collapsed = !*resource_collapsed;
        let panel_focus = resource_panel.read(cx).focus_handle();
        if *resource_collapsed {
            if panel_focus.contains_focused(window, cx) {
                let stops = crate::ui::panel::focus::dock_stops(dock_area.read(cx), cx);
                let next = stops
                    .first()
                    .and_then(|&panel| dock_area.read(cx).panel(panel))
                    .map(|view| view.focus_handle(cx))
                    .unwrap_or_else(|| self.focus_handle.clone());
                window.focus(&next, cx);
            }
        } else {
            window.focus(&panel_focus, cx);
        }
        cx.notify();
    }

    /// Moves the Resource panel to the other window edge, keeping its width,
    /// collapsed state and focus.
    pub(super) fn on_action_move_resource_panel(
        &mut self,
        _: &MoveResourcePanel,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace {
            resource_panel,
            resource_side,
            ..
        } = &mut self.mode
        else {
            return;
        };
        *resource_side = resource_side.other();
        let side = *resource_side;
        resource_panel.update(cx, |panel, cx| panel.set_side(side, cx));
        cx.notify();
    }

    /// Makes this window's current edge the one new windows open their panel on.
    /// This window, and every other open one, stays as it is.
    pub(super) fn on_action_save_resource_side(
        &mut self,
        _: &SaveResourceSide,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let WindowMode::Workspace { resource_side, .. } = &self.mode {
            crate::ui::resource_panel::set_preferred_side(*resource_side, cx);
        }
    }

    /// Test-only: which edge the Resource panel is on, and whether it's collapsed.
    #[cfg(test)]
    pub(crate) fn test_resource_layout(&self) -> Option<(ResourceSide, bool)> {
        match &self.mode {
            WindowMode::Workspace {
                resource_side,
                resource_collapsed,
                ..
            } => Some((*resource_side, *resource_collapsed)),
            WindowMode::Picker(_) => None,
        }
    }
}

#[cfg(test)]
mod tests;
