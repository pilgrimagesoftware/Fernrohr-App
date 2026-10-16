//! The window side of the exec library (`exec-command-library` D5): which
//! context Run uses when the library is opened from a cluster panel.

use super::{MainWindow, WindowMode};
use gpui_kit::component::Root;
use gpui_kit::*;

impl MainWindow {
    /// The context of the panel with keyboard focus (anywhere inside it), or
    /// `None` when focus is on no cluster panel.
    pub(super) fn focused_context(&self, window: &Window, cx: &App) -> Option<String> {
        let WindowMode::Workspace {
            open_panels,
            dock_area,
            ..
        } = &self.mode
        else {
            return None;
        };
        let area = dock_area.read(cx);
        open_panels
            .iter()
            .find(|open| {
                area.panel(open.id)
                    .is_some_and(|view| view.focus_handle(cx).contains_focused(window, cx))
            })
            .map(|open| open.key.context_name.clone())
    }
}

/// The context of the focused cluster panel in the main window `window` holds.
pub(crate) fn focused_context(window: &mut Window, cx: &App) -> Option<String> {
    let Some(Some(root)) = window.root::<Root>() else {
        return None;
    };
    let main_window = root.read(cx).view().clone().downcast::<MainWindow>().ok()?;
    main_window.read(cx).focused_context(window, cx)
}
