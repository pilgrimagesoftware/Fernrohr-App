//! Window geometry and restore: turning a saved `WindowLayout` into a window's bounds, contexts and dock, and recording a closing window's layout.

use super::*;

/// Descriptors this build knows how to restore, in persisted order. A
/// descriptor whose `kind` this build doesn't recognize deserializes as
/// [`PanelDescriptor::Unknown`] (see `config::workspace`) and is skipped here
/// with a log line, rather than failing the whole layout.
pub fn restorable_panels(layout: &WindowLayout) -> Vec<&PanelDescriptor> {
    layout
        .panels
        .iter()
        .filter(|descriptor| match descriptor {
            PanelDescriptor::Unknown => {
                log::warn!("skipping unknown panel kind on workspace restore");
                false
            }
            _ => true,
        })
        .collect()
}

pub(super) fn window_bounds(layout: &WindowLayout, cx: &mut App) -> Bounds<Pixels> {
    let size = size(px(layout.width), px(layout.height));
    match (layout.x, layout.y) {
        (Some(x), Some(y)) => Bounds::new(Point::new(px(x), px(y)), size),
        _ => Bounds::centered(None, size, cx),
    }
}

/// The geometry a save records, from a live window's outer `frame`
/// (`window.bounds()`) and its `content` size (`window.viewport_size()`): the
/// frame's top-left origin, which is where `open_window` places the frame, with
/// the content size, which is what `WindowBounds::Windowed` sizes. On macOS the
/// frame includes the title bar, so saving its size grew each restored window
/// by the title bar's height on every relaunch (Fernrohr#51).
pub(super) fn restorable_bounds(frame: Bounds<Pixels>, content: Size<Pixels>) -> Bounds<Pixels> {
    Bounds::new(frame.origin, content)
}

/// A live window's [`WindowLayout`], as both save sites record it: its
/// [`restorable_bounds`] and [`workspace_contexts`].
pub(super) fn layout_from_window(window: &mut Window, cx: &mut App) -> WindowLayout {
    let bounds = restorable_bounds(window.bounds(), window.viewport_size());
    let live = workspace_contexts(window, cx);
    layout_from_bounds(bounds, live)
}

/// Task 2.2's save side: `contexts` is the live window's own `WindowMode::Workspace`
/// list (empty for a `Picker`-mode window, which has none yet), read by
/// [`workspace_contexts`] just before this is called. `panels` stays empty - turning
/// a window's actual open panels into `PanelDescriptor`s is a later change's job; on
/// restore, `contexts` alone is enough to reconnect every context a window used
/// (the spec's "Multi-context window restored" scenario), even before that job lands.
pub(super) fn layout_from_bounds(bounds: Bounds<Pixels>, live: LiveWorkspace) -> WindowLayout {
    WindowLayout {
        width: f32::from(bounds.size.width),
        height: f32::from(bounds.size.height),
        x: Some(f32::from(bounds.origin.x)),
        y: Some(f32::from(bounds.origin.y)),
        contexts: live.contexts,
        panels: Vec::new(),
        resource_panel_width: live.resource_width.map(f32::from),
    }
}

/// What a save reads off a live window beyond its geometry.
#[derive(Default)]
pub(super) struct LiveWorkspace {
    pub(super) contexts: Vec<String>,
    pub(super) resource_width: Option<Pixels>,
}

/// The live contexts a window uses, read back through its `Root` - empty for a
/// `Picker`-mode window (nothing to save yet) or one whose `Root`/`MainWindow` can't
/// be found (shouldn't happen for a window this module opened, but geometry alone is
/// still worth saving over failing the whole snapshot).
pub(super) fn workspace_contexts(window: &mut Window, cx: &mut App) -> LiveWorkspace {
    let Some(Some(root)) = window.root::<Root>() else {
        return LiveWorkspace::default();
    };
    let Ok(main_window) = root.read(cx).view().clone().downcast::<MainWindow>() else {
        return LiveWorkspace::default();
    };
    let main_window = main_window.read(cx);
    LiveWorkspace {
        contexts: main_window.contexts(),
        resource_width: main_window.resource_width(),
    }
}

/// Refreshes the saved dock arrangement of the main window `window` holds, from
/// its dock as last drawn - see [`MainWindow::save_dock_layout`] for why the
/// `LayoutChanged` save alone is not enough. A no-op for any other window.
pub(super) fn save_window_dock_layout(window: &mut Window, cx: &mut App) {
    let Some(Some(root)) = window.root::<Root>() else {
        return;
    };
    let Ok(main_window) = root.read(cx).view().clone().downcast::<MainWindow>() else {
        return;
    };
    main_window.update(cx, |main_window, cx| main_window.save_dock_layout(cx));
}

/// Remembers a closing main window's layout so `save` still writes it after the window
/// is gone (see [`ClosedWindowLayouts`]).
///
/// Only the *last* main window is recorded. A window closed while others stay open
/// is one the user is done with, so it's dropped rather than restored at the next
/// launch. The last one is recorded because closing it quits the app
/// (`QuitMode::LastWindowClosed`) before `save` can read any open window.
pub(super) fn record_closing_layout(window_id: WindowId, window: &mut Window, cx: &mut App) {
    if other_open_main_windows(window_id, cx) > 0 {
        if cx.has_global::<ClosedWindowLayouts>() {
            cx.global_mut::<ClosedWindowLayouts>().0.remove(&window_id);
        }
        return;
    }
    let layout = layout_from_window(window, cx);
    if !cx.has_global::<ClosedWindowLayouts>() {
        cx.set_global(ClosedWindowLayouts::default());
    }
    cx.global_mut::<ClosedWindowLayouts>()
        .0
        .insert(window_id, layout);
}

/// Whether `handle` is a main (workspace or picker) window - one with a
/// layout worth saving, unlike Settings, Tunnels or About.
pub(super) fn is_main_window(handle: AnyWindowHandle, cx: &App) -> bool {
    handle
        .downcast::<Root>()
        .and_then(|root| root.read(cx).ok())
        .is_some_and(|root| root.view().clone().downcast::<MainWindow>().is_ok())
}

/// How many main (workspace or picker) windows other than `except` are open.
pub(super) fn other_open_main_windows(except: WindowId, cx: &App) -> usize {
    cx.windows()
        .into_iter()
        .filter(|handle| handle.window_id() != except)
        .filter(|handle| is_main_window(*handle, cx))
        .count()
}

/// Closes `window` the way its close button does. `remove_window` skips the
/// platform's should-close hook, so a main window's layout is recorded here first;
/// other windows (Tunnels, About) have no layout to keep.
pub(crate) fn close_window(window: &mut Window, cx: &mut App) {
    let is_main = window.root::<Root>().flatten().is_some_and(|root| {
        root.read(cx)
            .view()
            .clone()
            .downcast::<MainWindow>()
            .is_ok()
    });
    if is_main {
        record_closing_layout(window.window_handle().window_id(), window, cx);
    }
    window.remove_window();
}

/// How many contexts the window currently uses, read live from its `MainWindow` - 1
/// for a picker-mode window or one this module didn't open. Read, not cached: the
/// count changes on add and disconnect, and dock panels are drawn after
/// `MainWindow`'s own render has returned, so reading it here never re-enters it.
pub(crate) fn window_context_count(window: &mut Window, cx: &App) -> usize {
    let Some(Some(root)) = window.root::<Root>() else {
        return 1;
    };
    let Ok(main_window) = root.read(cx).view().clone().downcast::<MainWindow>() else {
        return 1;
    };
    main_window.read(cx).contexts().len().max(1)
}

/// A saved Resource panel width, clamped to the range its divider allows; the
/// default when the layout has none.
pub(super) fn restored_resource_width(layout: &WindowLayout) -> Pixels {
    layout
        .resource_panel_width
        .map(px)
        .map(|width| width.clamp(RESOURCE_PANEL_MIN_WIDTH, RESOURCE_PANEL_MAX_WIDTH))
        .unwrap_or(RESOURCE_PANEL_WIDTH)
}

/// Builds the workspace dock for `context_name` and opens its first panel:
/// Pods, the default view for a freshly connected cluster. Every later panel
/// goes through `MainWindow::open_target`, which reuses this dock - and the
/// window's `ClusterSession` - rather than rebuilding either.
pub(super) fn build_workspace(
    context_name: String,
    connection_count: usize,
    window: &mut Window,
    cx: &mut App,
) -> (
    Entity<DockArea>,
    Rc<DockSkin>,
    PanelScope,
    (PanelId, nav::OpenedPanel),
) {
    let (dock_area, dock_skin) = DockSkin::dock_area("main", Some(1), window, cx);
    // Every tab carries its own close control (`tab-close-buttons` 1.1);
    // gpui-kit 0.7 draws it only when asked.
    dock_skin.set_close_button_visible(true, cx);
    let scope = PanelScope {
        connection_count,
        ..PanelScope::new(NavTarget::pods(), context_name)
    };
    // Returned so the window records it in `open_panels` like any other: a
    // window that opened Pods at startup must still recognise Pods as open,
    // or the first click on the Pod row would duplicate it (spec 9.3). The
    // scope comes back too, so the key is derived from the very scope the panel
    // was built with rather than restated beside it.
    let first = dock_area.update(cx, |area, cx| {
        nav::add_panel(area, &scope, None, window, cx)
    });
    (dock_area, dock_skin, scope, first)
}

/// Every context a restored (non-empty) layout's workspace should reconnect, in the
/// order the window used them: `layout.contexts` when it says anything, otherwise
/// (`window-context-bar` design.md decision 3, an older file with no `contexts` at
/// all) the distinct `cluster_context`s named by its saved panels, first-seen order.
/// Empty for a layout with no restorable panels - that window opens in `Picker` mode.
pub(super) fn restored_contexts(layout: &WindowLayout) -> Vec<String> {
    if !layout.contexts.is_empty() {
        return layout.contexts.clone();
    }
    let mut contexts = Vec::new();
    for descriptor in restorable_panels(layout) {
        let context_name = match descriptor {
            PanelDescriptor::Pods {
                cluster_context, ..
            }
            | PanelDescriptor::Logs {
                cluster_context, ..
            } => cluster_context,
            PanelDescriptor::Unknown => continue,
        };
        if !contexts.contains(context_name) {
            contexts.push(context_name.clone());
        }
    }
    contexts
}

#[cfg(test)]
mod tests;
