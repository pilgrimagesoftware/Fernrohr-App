//! `MainWindow`'s handlers for Focus Next / Previous Panel. The window runs
//! them because it owns both the Resource panel and the dock; the stop order
//! and stepping live in [`crate::ui::panel::focus`].

use super::{MainWindow, WindowMode};
use crate::ui::panel::focus::{self, Direction, FocusNextPanel, FocusPreviousPanel};
use gpui_kit::{Context, Window};

impl MainWindow {
    /// `panel.focus_next` / `panel.focus_previous`: steps keyboard focus
    /// through this window's panels - see [`crate::ui::panel::focus`].
    pub(super) fn on_action_focus_next_panel(
        &mut self,
        _: &FocusNextPanel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_panel_focus(Direction::Next, window, cx);
    }

    pub(super) fn on_action_focus_previous_panel(
        &mut self,
        _: &FocusPreviousPanel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_panel_focus(Direction::Previous, window, cx);
    }

    fn move_panel_focus(
        &mut self,
        direction: Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let WindowMode::Workspace {
            dock_area,
            resource_panel,
            resource_collapsed,
            ..
        } = &self.mode
        {
            // A collapsed Resource panel isn't drawn, so it isn't a stop.
            let resource = (!*resource_collapsed).then(|| resource_panel.read(cx).focus_handle());
            focus::move_focus(resource, dock_area, direction, window, cx);
        }
    }
}

#[cfg(test)]
mod tests;
