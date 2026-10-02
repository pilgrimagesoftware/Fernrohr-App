//! `MainWindow`'s handlers for Focus Next / Previous Panel. The window runs
//! them because it owns both the Resource panel and the dock; the stop order
//! and stepping live in [`crate::ui::panel::focus`].
//!
//! [`register_unfocused_fallbacks`] covers a window with nothing focused, where
//! a key dispatches past `MainWindow` - which only hears actions on its focus
//! path - to the app's own listeners.

use super::{MainWindow, WindowMode};
use crate::ui::panel::focus::{self, Direction, FocusNextPanel, FocusPreviousPanel};
use crate::ui::resource_panel::FocusResources;
use gpui_kit::component::Root;
use gpui_kit::{App, Context, Window};

/// App-level listeners for the commands that put focus back on a panel - Focus
/// Next / Previous Panel and Focus Resources. They only run when no element on
/// the focus path took the action, which with a `MainWindow` open means nothing
/// in its window has focus; they hand it to the active window's `MainWindow`.
pub(super) fn register_unfocused_fallbacks(cx: &mut App) {
    cx.on_action(|_: &FocusNextPanel, cx: &mut App| {
        in_active_main_window(cx, |main, window, cx| {
            main.move_panel_focus(Direction::Next, window, cx)
        })
    });
    cx.on_action(|_: &FocusPreviousPanel, cx: &mut App| {
        in_active_main_window(cx, |main, window, cx| {
            main.move_panel_focus(Direction::Previous, window, cx)
        })
    });
    cx.on_action(|_: &FocusResources, cx: &mut App| {
        in_active_main_window(cx, |main, window, cx| {
            main.on_action_focus_resources(&FocusResources, window, cx)
        })
    });
}

/// Runs `f` on the active window's `MainWindow`, if it is one. Deferred: the
/// listener runs while that window is mid-dispatch, where updating it fails.
fn in_active_main_window(
    cx: &mut App,
    f: impl FnOnce(&mut MainWindow, &mut Window, &mut Context<MainWindow>) + 'static,
) {
    let Some(handle) = cx.active_window() else {
        return;
    };
    cx.defer(move |cx| {
        let _ = handle.update(cx, |_, window, cx| {
            let Some(Some(root)) = window.root::<Root>() else {
                return;
            };
            let Ok(main) = root.read(cx).view().clone().downcast::<MainWindow>() else {
                return;
            };
            main.update(cx, |main, cx| f(main, window, cx));
        });
    });
}

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

    pub(super) fn move_panel_focus(
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
