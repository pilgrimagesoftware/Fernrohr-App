//! The windows `agent-mcp`'s navigation tools (Fernrohr#189 section 4) act
//! on, and the two things they do there: open or focus a panel, and load a
//! saved layout. Both run on the main thread (the tools reach it through
//! `mcp::foreground`) and go through the same [`MainWindow`] methods the
//! app's own commands do - [`MainWindow::open_target_in`] and
//! [`MainWindow::load_layout`] - so an agent's request dedups, places and
//! restores exactly as the user's would.
//!
//! The agent's terminal, not Fernrohr, usually has focus when a request
//! arrives, so "the focused window" is the frontmost main window rather than
//! only the platform's active one.

use super::open::ShownPanel;
use super::*;
use crate::config::saved_layouts::SavedLayout;

/// Why a navigation request found nowhere to land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NavigateError {
    /// No main window is showing a workspace.
    NoWindow,
    /// No workspace window holds the requested context.
    ContextNotHeld,
}

/// Opens or focuses the panel for `target` in `context_name`, with
/// `namespaces` as its scope (empty for the context's default, as an in-app
/// open with no scope gets), in the frontmost workspace window holding that
/// context. Every other window is left as it was.
pub(crate) fn open_panel(
    context_name: String,
    target: NavTarget,
    namespaces: Vec<String>,
    cx: &mut App,
) -> Result<ShownPanel, NavigateError> {
    let windows = workspace_windows(cx);
    if windows.is_empty() {
        return Err(NavigateError::NoWindow);
    }
    let (handle, main) = windows
        .into_iter()
        .find(|(_, main)| main.read(cx).contexts().contains(&context_name))
        .ok_or(NavigateError::ContextNotHeld)?;
    handle
        .update(cx, |_, window, cx| {
            main.update(cx, |main, cx| {
                main.open_target_in(
                    target,
                    None,
                    Some(context_name),
                    namespaces,
                    OpenMode::Foreground,
                    window,
                    cx,
                )
            })
        })
        // The window closed between the lookup and the update.
        .map_err(|_| NavigateError::NoWindow)?
        .ok_or(NavigateError::ContextNotHeld)
}

/// Loads `layout` in `mode` into the frontmost workspace window; `None` when
/// no window shows a workspace.
pub(crate) fn load_layout(
    layout: SavedLayout,
    mode: LoadMode,
    cx: &mut App,
) -> Option<LoadedLayout> {
    let (handle, main) = workspace_windows(cx).into_iter().next()?;
    handle
        .update(cx, |_, window, cx| {
            main.update(cx, |main, cx| main.load_layout(layout, mode, window, cx))
        })
        .ok()
}

/// Every main window showing a workspace, frontmost first: the platform's
/// active window, then the on-screen stacking order where the platform
/// reports one, then the rest in the order they opened.
fn workspace_windows(cx: &App) -> Vec<(AnyWindowHandle, Entity<MainWindow>)> {
    let ordered = cx
        .active_window()
        .into_iter()
        .chain(cx.window_stack().unwrap_or_default())
        .chain(cx.windows());
    let mut seen = Vec::new();
    ordered
        .filter(|handle| {
            let first = !seen.contains(&handle.window_id());
            seen.push(handle.window_id());
            first
        })
        .filter_map(|handle| {
            let root = handle.downcast::<Root>()?.read(cx).ok()?;
            let main = root.view().clone().downcast::<MainWindow>().ok()?;
            matches!(main.read(cx).mode, WindowMode::Workspace { .. }).then_some((handle, main))
        })
        .collect()
}
