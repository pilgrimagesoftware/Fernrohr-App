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
//! only the platform's active one. A tool that changes what a window shows
//! then brings Fernrohr forward with that window focused ([`focus_window`],
//! `mcp-connect-and-focus` D4), so the user sees what the agent showed them.

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
    let shown = handle
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
        .ok_or(NavigateError::ContextNotHeld)?;
    focus_window(handle, cx);
    Ok(shown)
}

/// Loads `layout` in `mode` into the frontmost workspace window; `None` when
/// no window shows a workspace.
pub(crate) fn load_layout(
    layout: SavedLayout,
    mode: LoadMode,
    cx: &mut App,
) -> Option<LoadedLayout> {
    let (handle, main) = workspace_windows(cx).into_iter().next()?;
    let loaded = handle
        .update(cx, |_, window, cx| {
            main.update(cx, |main, cx| main.load_layout(layout, mode, window, cx))
        })
        .ok()?;
    focus_window(handle, cx);
    Some(loaded)
}

/// `connect_context`'s already-open case (`mcp-connect-and-focus` D1): brings
/// the frontmost workspace window forward if it holds `context_name`, and says
/// whether it did. `false` when it doesn't hold it, or no window shows a
/// workspace.
pub(crate) fn focus_if_held(context_name: &str, cx: &mut App) -> bool {
    let Some((handle, main)) = workspace_windows(cx).into_iter().next() else {
        return false;
    };
    if !main
        .read(cx)
        .contexts()
        .iter()
        .any(|held| held == context_name)
    {
        return false;
    }
    focus_window(handle, cx);
    true
}

/// Connects `context_name` for an agent, once the user has allowed it: adds it
/// to the frontmost workspace window exactly as the status bar's add-context
/// control does ([`MainWindow::add_context`] - tunnel binding, a shared
/// connection, its Pods panel, selected), or opens a window on it when none
/// shows a workspace, then brings that window forward.
pub(crate) fn connect_context(context_name: String, cx: &mut App) {
    let handle = match workspace_windows(cx).into_iter().next() {
        Some((handle, main)) => {
            let _ = handle.update(cx, |_, window, cx| {
                main.update(cx, |main, cx| main.add_context(context_name, window, cx))
            });
            handle
        }
        None => open_window(
            cx,
            WindowLayout {
                contexts: vec![context_name],
                ..Default::default()
            },
        ),
    };
    focus_window(handle, cx);
}

/// Brings Fernrohr to the front with `window` focused - what a click on one of
/// its notifications does too (`notify::focus`). A window that has closed is
/// left alone.
pub(crate) fn focus_window(window: AnyWindowHandle, cx: &mut App) {
    cx.activate(true);
    let _ = window.update(cx, |_, window, _| window.activate_window());
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
