//! Saving every open window's layout at quit, and reopening them at start.

use super::*;
use crate::consts::BOUNDS_SAVE_DEBOUNCE;

/// Opens the windows recorded at `workspace_path`, or one default window if
/// the file is missing, empty, or failed to parse (`config::load` already
/// guarantees defaults-without-touching-the-file in that last case).
///
/// Then brings the app to the front with its first window active: a launch from
/// Finder, the Dock or a terminal otherwise left the window behind whatever
/// was frontmost, with keyboard input still going there.
pub fn open_saved_or_default(cx: &mut App, workspace_path: &Path) {
    let workspace: WorkspaceConfig = config::load(workspace_path);
    super::namespace_defaults::NamespaceDefaults::load(cx, workspace.namespace_defaults.clone());
    let layouts = if workspace.windows.is_empty() {
        vec![WindowLayout::default()]
    } else {
        workspace.windows
    };
    let windows: Vec<AnyWindowHandle> = layouts
        .into_iter()
        .map(|layout| open_window(cx, layout))
        .collect();
    cx.activate(true);
    if let Some(first) = windows.first() {
        let _ = first.update(cx, |_, window, _| window.activate_window());
    }
}

/// Snapshots every window's geometry into `workspace_path`: still-open
/// windows read fresh from `cx.windows()`, plus any already closed this run
/// (see [`ClosedWindowLayouts`]) - under `QuitMode::LastWindowClosed` that's
/// every window, since the app-quit callback fires after the last one
/// closes. Panel descriptors are left empty until a later change adds panel
/// kinds worth restoring (see [`open_window`]'s placeholder split).
///
/// Each open main window's dock arrangement is refreshed into
/// [`SavedDockLayouts`] on the way, from the dock as last drawn, so the quit
/// that writes those layouts out writes the sizes on screen.
pub fn save(cx: &mut App, workspace_path: &Path) {
    let mut layouts = if cx.has_global::<ClosedWindowLayouts>() {
        cx.global::<ClosedWindowLayouts>().0.clone()
    } else {
        HashMap::new()
    };
    // Main windows only: a Settings, Tunnels or About window open at quit
    // would otherwise be written as an extra, context-less main window.
    for handle in cx.windows() {
        if !is_main_window(handle, cx) {
            continue;
        }
        if let Ok(layout) = handle.update(cx, |_, window, cx| {
            save_window_dock_layout(window, cx);
            layout_from_window(window, cx)
        }) {
            layouts.insert(handle.window_id(), layout);
        }
    }
    let windows: Vec<WindowLayout> = layouts.into_values().collect();
    let namespace_defaults = super::namespace_defaults::NamespaceDefaults::snapshot(cx);
    let _ = config::save(
        workspace_path,
        &WorkspaceConfig {
            windows,
            namespace_defaults,
        },
    );
}

/// Saves the workspace once windows stop moving or resizing for
/// [`BOUNDS_SAVE_DEBOUNCE`], so a kill or crash - which never reaches the
/// quit-time [`save`] - doesn't lose the layout. Each call replaces (and so
/// cancels) the pending save. A no-op before [`init`] has recorded the path.
pub(super) fn schedule_save(cx: &mut App) {
    let Some(path) = cx.try_global::<WorkspacePath>().map(|path| path.0.clone()) else {
        return;
    };
    let task = cx.spawn(async move |cx| {
        cx.background_executor().timer(BOUNDS_SAVE_DEBOUNCE).await;
        cx.update(|cx| save(cx, &path));
    });
    cx.set_global(PendingSave { _task: task });
}

#[cfg(test)]
mod tests;
