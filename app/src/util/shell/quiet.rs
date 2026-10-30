//! Quiet mode (#236, macOS): closing the last window no longer quits. The app
//! disconnects everything it had connected, as quitting would, and runs on in
//! the menu bar - its MCP server still serving unless the background setting
//! (`ui::background_mcp`) says to stop it. Quit stays the only way out.
//!
//! - [`enter`]: the last window closed. Each window's holds were already
//!   released as it closed, which drops every session - its connection,
//!   watches and tunnel - so what's left is the app-wide port-forwards and any
//!   `ssh`/command forward still alive, both stopped here, and the MCP
//!   server, stopped only with the setting off. The workspace is saved now,
//!   not only at quit, so a kill while quiet loses nothing.
//! - [`leave`]: a main window is opening. It starts the MCP server again if it
//!   was stopped, and forgets the closed window's layout: from here the open
//!   windows are what quit saves.
//! - [`reopen`]: the Dock icon, a status menu item or an agent needs a window.
//!   It opens the main window that closed last, as launch restores one, or a
//!   default window when there's none.
//!
//! On Linux and Windows the app still quits with its last window (#336 is
//! their tray), so none of this is installed there; [`QUIET_MODE`] says which.

use super::*;
use crate::config::ui::BackgroundMcp;
use crate::k8s::cluster::port_forwards::PortForwards;

/// Whether this platform keeps running with no window open (macOS), rather
/// than quitting with the last one.
pub const QUIET_MODE: bool = cfg!(target_os = "macos");

/// How the app quits on this platform: only when asked, with quiet mode;
/// with its last window otherwise.
pub fn quit_mode() -> QuitMode {
    if QUIET_MODE {
        QuitMode::Explicit
    } else {
        QuitMode::LastWindowClosed
    }
}

/// Opens a main window connected to `context` - what a context in the status
/// menu's Contexts submenu does.
#[derive(Clone, Debug, PartialEq, Eq, Action)]
#[action(namespace = shell, no_json)]
pub struct OpenContextWindow {
    pub context: String,
}

/// What quiet mode does outside the app's own state: start and stop the MCP
/// endpoint, and kill this run's leftover forwards. The real ones in the app;
/// stand-ins in tests, which must neither bind the endpoint nor sweep the
/// user's pidfile directory.
#[derive(Clone, Copy)]
pub struct Hooks {
    pub start_mcp: fn(&mut App),
    pub stop_mcp: fn(&mut App),
    pub kill_forwards: fn(),
}

impl Hooks {
    /// The app's own: the MCP endpoint and `util::pidfile`'s quit-time kill.
    pub fn app() -> Self {
        Self {
            start_mcp: crate::mcp::start,
            stop_mcp: crate::mcp::stop,
            kill_forwards: || {
                crate::util::pidfile::kill_live_forwards();
            },
        }
    }
}

/// Whether the app is quiet now, and what it does on the way in and out.
struct Quiet {
    active: bool,
    hooks: Hooks,
}

impl Global for Quiet {}

/// Installs quiet mode from `main`, after `util::shell::init`, whose own
/// window-closed observer releases a closing window's holds first: the
/// app-wide handlers, and - where [`QUIET_MODE`] holds - entering quiet mode
/// when the last window closes.
pub fn init(hooks: Hooks, cx: &mut App) {
    register_handlers(hooks, cx);
    if QUIET_MODE {
        install(cx);
    }
}

/// The app-wide handlers, and the hooks quiet mode uses.
pub(super) fn register_handlers(hooks: Hooks, cx: &mut App) {
    cx.set_global(Quiet {
        active: false,
        hooks,
    });
    cx.on_action(|action: &OpenContextWindow, cx: &mut App| {
        open_context_window(&action.context, cx);
    });
}

/// Enters quiet mode whenever the last window closes. Apart from [`init`] so
/// tests can install it on any platform.
pub(super) fn install(cx: &mut App) {
    cx.on_window_closed(|cx, _| {
        // GPUI drops the closed window from `cx.windows()` before observers run.
        if cx.windows().is_empty() {
            enter(cx);
        }
    })
    .detach();
}

/// Whether the app is in quiet mode: running with no window.
pub fn is_quiet(cx: &App) -> bool {
    cx.try_global::<Quiet>().is_some_and(|quiet| quiet.active)
}

/// The hooks [`init`] was given, or none outside quiet mode.
fn hooks(cx: &App) -> Option<Hooks> {
    cx.try_global::<Quiet>().map(|quiet| quiet.hooks)
}

/// The last window closed: disconnect what's left, save the workspace, and
/// stop the MCP server if the setting says to.
pub(super) fn enter(cx: &mut App) {
    if let Some(forwards) = PortForwards::existing(cx) {
        forwards.update(cx, |forwards, cx| {
            forwards.stop_all(cx);
        });
    }
    let Some(hooks) = hooks(cx) else {
        return;
    };
    // Every session is already gone with its last holder, and its forward with
    // it; this catches one anything else still held, as quitting does.
    (hooks.kill_forwards)();
    if let Some(path) = cx.try_global::<WorkspacePath>().map(|path| path.0.clone()) {
        save(cx, &path);
    }
    if crate::ui::background_mcp::current(cx) == BackgroundMcp::StopWithLastWindow {
        (hooks.stop_mcp)(cx);
    }
    if cx.has_global::<Quiet>() {
        cx.global_mut::<Quiet>().active = true;
    }
    log::info!("last window closed; running in the menu bar");
}

/// A main window is about to open: the app leaves quiet mode, starting the MCP
/// server again if it was stopped, and the window that closed last is no
/// longer what quit restores. A no-op unless quiet.
pub(super) fn leave(cx: &mut App) {
    if !is_quiet(cx) {
        return;
    }
    cx.global_mut::<Quiet>().active = false;
    if cx.has_global::<ClosedWindowLayouts>() {
        cx.global_mut::<ClosedWindowLayouts>().0.clear();
    }
    if let Some(hooks) = hooks(cx) {
        (hooks.start_mcp)(cx);
    }
}

/// Opens the main window that closed last - launch's restore of it - or a
/// default window when none has, and brings the app forward with it focused.
/// For the Dock icon, the status menu and an agent that needs a window.
pub fn reopen(cx: &mut App) -> AnyWindowHandle {
    // Only the last main window to close is recorded (`record_closing_layout`),
    // and a main window opening forgets it (`leave`), so there is at most one.
    let layout = if cx.has_global::<ClosedWindowLayouts>() {
        let closed = &mut cx.global_mut::<ClosedWindowLayouts>().0;
        let last = closed.keys().next().copied();
        last.and_then(|id| closed.remove(&id))
    } else {
        None
    }
    .unwrap_or_default();
    let handle = open_window(cx, layout);
    bring_forward(handle, cx);
    handle
}

/// Opens a main window connected to `context`, and brings it forward.
pub fn open_context_window(context: &str, cx: &mut App) -> AnyWindowHandle {
    let handle = open_window(
        cx,
        WindowLayout {
            contexts: vec![context.to_string()],
            ..WindowLayout::default()
        },
    );
    bring_forward(handle, cx);
    handle
}

/// A main window if one is open, otherwise one [`reopen`]ed - for an agent's
/// request that needs a window to show itself in (`mcp-connect-and-focus`).
pub fn ensure_main_window(cx: &mut App) {
    let any_main = cx
        .windows()
        .into_iter()
        .any(|handle| is_main_window(handle, cx));
    if !any_main {
        reopen(cx);
    }
}

fn bring_forward(handle: AnyWindowHandle, cx: &mut App) {
    cx.activate(true);
    let _ = handle.update(cx, |_, window, _| window.activate_window());
}

#[cfg(test)]
mod tests;
