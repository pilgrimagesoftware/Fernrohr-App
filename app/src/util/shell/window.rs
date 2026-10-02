//! A window's two modes - the cluster picker or a connected workspace - and opening a window and wiring up the view it starts in.

use super::*;

/// A window's body: the cluster picker (no connected context yet) or a connected
/// workspace. A window opens in `Picker` whenever it has no restored panels, per the
/// `cluster-picker` and `app-shell` specs.
pub(super) enum WindowMode {
    Picker(Entity<crate::ui::picker::ClusterPicker>),
    Workspace {
        dock_area: Entity<DockArea>,
        /// Keeps the dock's renderer alive. The default skin uses GPUI focus
        /// state for active panel chrome and provides its zoom control.
        _dock_skin: Rc<DockSkin>,
        /// Every context this window uses, in the order they were added
        /// (`window-context-bar` design.md decision 1). A restored window can
        /// already carry more than one; adding a second interactively (the "+"
        /// control, section 3) is still that change's own later sections' job.
        /// `connection_count` (how many cluster connections the window holds,
        /// which section 10.1's title bar reads) is always `contexts.len()`
        /// rather than a field of its own, so the two can never disagree.
        contexts: Vec<String>,
        /// Index into `contexts` naming which one the Resource panel's cluster
        /// dropdown currently shows, and which one a window-wide action (like
        /// `SetContextTunnel`) applies to. Always `0` while `contexts` has one
        /// entry.
        active: usize,
        /// The discovered-kind list in the window's left edge. It reads the
        /// same `ClusterSession` as `dock_area`, so picking a kind opens a
        /// panel without reconnecting.
        resource_panel: Entity<crate::ui::resource_panel::ResourcePanel>,
        /// Every panel this window has opened, in the order it opened them.
        /// The dock does not report which panel a `PanelId` belongs to, so
        /// this is also how a closed panel is recognised as closed.
        open_panels: Vec<OpenPanel>,
        /// What the Resource panel's row should mark. The window's most
        /// recently opened or focused kind, which is not the same as the
        /// dock's active panel once the user clicks tabs directly (section 12
        /// tracks that).
        nav: Box<NavTarget>,
        /// `connection-status-bar`: one item per context this window uses, shown along
        /// the workspace's bottom edge. Absent in `Picker` mode - the picker already
        /// shows its own connect progress (proposal.md's non-goals).
        status_bar: Entity<crate::ui::status_bar::StatusBarView>,
        /// `window-context-bar` section 3: one chip per context this window uses,
        /// shown along the workspace's top edge, below the title bar. `contexts` and
        /// `active` above are this bar's source of truth - every edit to either goes
        /// through [`MainWindow::sync_context_children`], which is what keeps the bar,
        /// the status bar, and the Resource panel's dropdown from disagreeing.
        context_bar: Entity<ContextBarView>,
        /// The Resource panel's current width: seeded from the saved layout, updated
        /// on every divider drag, and written back by [`save`].
        resource_width: Pixels,
    },
}

/// Opens one window, in `Picker` mode if `layout` has no restorable contexts, or
/// directly into a connected workspace (seeded from the restored layout's contexts)
/// otherwise.
pub fn open_window(cx: &mut App, layout: WindowLayout) {
    let bounds = window_bounds(&layout, cx);
    let contexts = restored_contexts(&layout);
    let resource_width = restored_resource_width(&layout);
    // Fully qualified: `util::shell` re-exports this function as `open_window`.
    gpui_kit::open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            // A restored layout can carry a window smaller than the picker needs
            // - it is whatever size the user last left it, from before the logo
            // grew. Without a floor the picker's centred column overflows and the
            // logo is clipped off the top.
            window_min_size: Some(crate::ui::picker::MIN_WINDOW_SIZE),
            // `Some` both sets the title and keeps it visible: with `None`,
            // macOS hides the title bar's text at creation and never revisits
            // it (`window-title-and-menu` design.md decisions 2 and 3).
            titlebar: Some(TitlebarOptions {
                title: Some(window_title::initial_title(&contexts).into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            ..Default::default()
        },
        cx,
        |window, cx| {
            crate::ui::theme::watch_window(window, cx);

            let window_id = window.window_handle().window_id();
            window.on_window_should_close(cx, move |window, cx| {
                record_closing_layout(window_id, window, cx);
                true
            });

            let view = cx.new(|cx| {
                let mut view = MainWindow {
                    mode: WindowMode::Picker(
                        cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
                    ),
                    focus_handle: cx.focus_handle(),
                };
                let WindowMode::Picker(picker) = &view.mode else {
                    unreachable!("just constructed a picker-mode window")
                };
                watch_picker(picker, window, cx);
                cx.observe_window_bounds(window, |_, _, cx| schedule_save(cx))
                    .detach();
                crate::ui::accent::watch_activation(window, cx);
                if !contexts.is_empty() {
                    view.enter_workspace(contexts, window, cx);
                    view.set_resource_width(resource_width);
                }
                view
            });
            view.update(cx, |view, cx| view.focus_initial(window, cx));
            view
        },
    )
    .expect("failed to open window");
}

/// Subscribes so a successful connect on `picker` swaps this window into
/// `Workspace` mode and arms [`watch_workspace`] on the new dock.
pub(super) fn watch_picker(
    picker: &Entity<crate::ui::picker::ClusterPicker>,
    window: &mut Window,
    cx: &mut Context<MainWindow>,
) {
    cx.subscribe_in(
        picker,
        window,
        |this: &mut MainWindow, _picker, event, window, cx| {
            let crate::ui::picker::PickerEvent::Connected { context_name, .. } = event;
            this.enter_workspace(vec![context_name.clone()], window, cx);
        },
    )
    .detach();
}

/// Subscribes so closing `dock_area`'s last panel swaps this window back into
/// `Picker` mode, per the `app-shell` spec's "closing the last panel returns
/// to the picker" scenario.
pub(super) fn watch_workspace(
    dock_area: &Entity<DockArea>,
    window: &mut Window,
    cx: &mut Context<MainWindow>,
) {
    cx.subscribe_in(
        dock_area,
        window,
        |this: &mut MainWindow, dock_area, event, window, cx| {
            if !matches!(event, DockEvent::LayoutChanged) {
                return;
            }
            // Task 2.2: keyed by every context the window uses (`context_lifecycle::
            // dock_layout_key`), not just the first - a single-context window's key is
            // still its bare context name, so this is the same save it always was for
            // that case, and a new one for a multi-context window (design.md decision 3
            // predates task 2.2's fuller persistence; see that task's own note on why).
            if let WindowMode::Workspace { contexts, .. } = &this.mode
                && cx.has_global::<SavedDockLayouts>()
            {
                let key = context_lifecycle::dock_layout_key(contexts);
                let state = dock_area.read(cx).dump(cx);
                cx.global_mut::<SavedDockLayouts>().0.insert(key, state);
            }
            this.forget_closed_panels(dock_area, cx);
            if !dock_area.read(cx).is_empty(DockPlacement::Center, cx) {
                return;
            }
            this.enter_picker(window, cx);
        },
    )
    .detach();
}

#[cfg(test)]
mod tests;
