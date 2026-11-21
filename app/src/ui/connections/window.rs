//! Owns the Connections window's singleton lifecycle (open-or-focus, as the
//! Tunnel Manager's) and its state: the selected row, the cached tunnel names,
//! and what keeps the list live.
//!
//! Live updates: the window observes the [`ClusterRegistry`] global, which
//! notifies on every hold, release and session change, and - since a state change
//! notifies only its own [`ClusterConnection`] - each listed context's
//! connection too, rebuilt whenever the registry changes. Tunnel names are read
//! only on construction and on [`TunnelsRevision`], never from `render`. No timer.
//!
//! Selection is by context name, so it stays on its row as others come and go;
//! when its row closes it moves to the row now in its place. It moves only with
//! the arrow keys and clicks - hover does nothing.

use super::rows::{ConnectionRow, TunnelNames, rows};
use crate::k8s::cluster::connection::ClusterConnection;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::tunnels::TunnelsRevision;
use gpui_kit::component::Root;
use gpui_kit::*;

/// The one Connections window, if open.
struct ConnectionsWindowHandle(Option<WindowHandle<Root>>);

impl Global for ConnectionsWindowHandle {}

/// Opens the Connections window, or brings the open one forward, with its list
/// focused. Reached from `connections.open` (palette, Window menu, status menu,
/// key) and from `connect_context` after an agent's approved connection.
pub(crate) fn open_or_focus(cx: &mut App) -> Option<WindowHandle<Root>> {
    if let Some(handle) = cx
        .try_global::<ConnectionsWindowHandle>()
        .and_then(|handle| handle.0)
        && cx.windows().contains(&handle.into())
    {
        let _ = handle.update(cx, |root, window, cx| {
            window.activate_window();
            if let Ok(view) = root.view().clone().downcast::<ConnectionsWindow>() {
                let focus = view.read(cx).focus_handle.clone();
                window.focus(&focus, cx);
            }
        });
        return Some(handle);
    }
    let options = WindowOptions {
        app_id: Some(crate::consts::APP_ID.into()),
        titlebar: Some(TitlebarOptions {
            title: Some("Connections".into()),
            ..Default::default()
        }),
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            crate::consts::CONNECTIONS_WINDOW_SIZE,
            cx,
        ))),
        window_min_size: Some(crate::consts::CONNECTIONS_WINDOW_MIN_SIZE),
        ..Default::default()
    };
    let (window, _) = gpui_kit::open_window(options, cx, |window, cx| {
        crate::ui::theme::watch_window(window, cx);
        let view = cx.new(ConnectionsWindow::new);
        let focus = view.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
        view
    })
    .ok()?;
    let handle = window.downcast::<Root>()?;
    let _ = handle.update(cx, |_, window, cx| {
        window.on_window_should_close(cx, |_, cx| {
            cx.set_global(ConnectionsWindowHandle(None));
            true
        });
    });
    cx.set_global(ConnectionsWindowHandle(Some(handle)));
    Some(handle)
}

/// Selects `context`'s row in the open Connections window, if it has one.
pub(crate) fn select_context(context: &str, cx: &mut App) {
    if let Some(view) = view(cx) {
        view.update(cx, |this, cx| this.select(context, cx));
    }
}

/// The open window's view.
fn view(cx: &mut App) -> Option<Entity<ConnectionsWindow>> {
    let handle = cx
        .try_global::<ConnectionsWindowHandle>()
        .and_then(|handle| handle.0)?;
    handle
        .update(cx, |root, _, _| {
            root.view().clone().downcast::<ConnectionsWindow>().ok()
        })
        .ok()
        .flatten()
}

/// Test-only: the open window's view.
#[cfg(test)]
pub(super) fn test_view(cx: &mut App) -> Option<Entity<ConnectionsWindow>> {
    view(cx)
}

/// Test-only: the open window.
#[cfg(test)]
pub(super) fn test_handle(cx: &App) -> Option<WindowHandle<Root>> {
    cx.try_global::<ConnectionsWindowHandle>()
        .and_then(|handle| handle.0)
}

pub(super) struct ConnectionsWindow {
    tunnels: TunnelNames,
    /// The selected row's context.
    pub(super) selected: Option<String>,
    /// The listed contexts as of the last change, in order: where a vanished
    /// selection's neighbour is.
    listed: Vec<String>,
    pub(super) focus_handle: FocusHandle,
    _connections: Vec<Subscription>,
    _subscriptions: Vec<Subscription>,
}

impl ConnectionsWindow {
    fn new(cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![
            cx.observe_global::<ClusterRegistry>(|this, cx| this.sync(cx)),
            cx.observe_global::<TunnelsRevision>(|this, cx| {
                this.load_tunnels();
                cx.notify();
            }),
        ];
        let mut this = Self {
            tunnels: TunnelNames::default(),
            selected: None,
            listed: Vec::new(),
            focus_handle: cx.focus_handle(),
            _connections: Vec::new(),
            _subscriptions: subscriptions,
        };
        this.load_tunnels();
        this.sync(cx);
        this
    }

    fn load_tunnels(&mut self) {
        let path = crate::util::paths::preference_dir().join("tunnels.toml");
        let (choices, bindings) = crate::ui::picker::load_tunnels(&path);
        self.tunnels = TunnelNames { choices, bindings };
    }

    /// The listed rows, read from the registry now.
    pub(super) fn rows(&self, cx: &App) -> Vec<ConnectionRow> {
        rows(cx, &self.tunnels)
    }

    /// The selected row, if one is.
    pub(super) fn selected_row(&self, cx: &App) -> Option<ConnectionRow> {
        let selected = self.selected.as_deref()?;
        self.rows(cx)
            .into_iter()
            .find(|row| row.context == selected)
    }

    /// The registry or a connection changed: watch the connections now listed,
    /// keep the selection on a row that exists, and redraw.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let rows = self.rows(cx);
        let connections: Vec<Entity<ClusterConnection>> = rows
            .iter()
            .filter_map(|row| ClusterRegistry::existing_connection(cx, &row.context))
            .collect();
        self._connections = connections
            .iter()
            .map(|connection| cx.observe(connection, |_, _, cx| cx.notify()))
            .collect();
        let contexts: Vec<String> = rows.into_iter().map(|row| row.context).collect();
        let still_listed = self
            .selected
            .as_ref()
            .is_some_and(|selected| contexts.contains(selected));
        if !still_listed {
            let at = self
                .selected
                .as_ref()
                .and_then(|selected| self.listed.iter().position(|c| c == selected))
                .unwrap_or(0);
            self.selected = contexts
                .get(at.min(contexts.len().saturating_sub(1)))
                .cloned();
        }
        self.listed = contexts;
        cx.notify();
    }

    /// Selects `context`'s row. Not checked against the list: a row just added
    /// reaches it on the registry's own notification, which comes after the
    /// caller's update, and a selection with no row falls back in [`Self::sync`].
    pub(super) fn select(&mut self, context: &str, cx: &mut Context<Self>) {
        self.selected = Some(context.to_string());
        cx.notify();
    }

    /// Moves the selection `delta` rows, within the list - the arrow keys.
    pub(super) fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.listed.is_empty() {
            return;
        }
        let at = self
            .selected
            .as_ref()
            .and_then(|selected| self.listed.iter().position(|c| c == selected))
            .unwrap_or(0) as isize;
        let next = (at + delta).clamp(0, self.listed.len() as isize - 1) as usize;
        self.selected = Some(self.listed[next].clone());
        cx.notify();
    }
}

impl Focusable for ConnectionsWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
