//! `MainWindow` itself: its state, initial focus, entering the workspace, and the scope and width bookkeeping panels report back through.

use super::*;

pub struct MainWindow {
    pub(super) mode: WindowMode,
    pub(super) focus_handle: FocusHandle,
}

impl MainWindow {
    pub(super) fn focus_initial(&self, window: &mut Window, cx: &mut App) {
        if let WindowMode::Picker(picker) = &self.mode {
            let picker = picker.clone();
            let focus_handle = picker.read(cx).initial_focus_handle(cx);
            focus_handle.focus(window, cx);
        } else {
            self.focus_displayed_panel(window, cx);
        }
    }

    /// Swaps this window to a connected workspace on `contexts`: the dock (defaulting
    /// to a Pods panel on `contexts[0]`), the Resource panel listing that cluster's
    /// discovered kinds, and the subscription that opens whatever is picked. Both
    /// windows-enter-workspace sites go through here so the Resource panel cannot be
    /// wired up in one of them and forgotten in the other.
    ///
    /// Takes this window's hold on every one of `contexts` (`window-context-bar`
    /// design.md decision 2) before building anything else, so a session already
    /// exists (or is connected here) for every step below to read - including a
    /// context past `contexts[0]`, which a restored multi-context window holds but,
    /// absent the "+" control and its panel (section 3), opens no panel for yet.
    ///
    /// `contexts` must not be empty.
    pub(super) fn enter_workspace(
        &mut self,
        contexts: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        debug_assert!(
            !contexts.is_empty(),
            "a workspace always has at least one context"
        );
        let window_id = window.window_handle().window_id();
        for context_name in &contexts {
            ClusterRegistry::hold(cx, context_name, window_id);
        }
        let context_name = contexts[0].clone();
        // Task 2.2: keyed by every context this window uses - see `watch_workspace`'s
        // matching save-side key.
        let saved_layout = if cx.has_global::<SavedDockLayouts>() {
            cx.global::<SavedDockLayouts>()
                .0
                .get(&context_lifecycle::dock_layout_key(&contexts))
                .cloned()
        } else {
            None
        };
        // One connection per context in `contexts`; see `WindowMode::Workspace::contexts`.
        let connection_count = contexts.len();
        let (dock_area, dock_skin, scope, (first_id, first)) =
            build_workspace(context_name.clone(), connection_count, window, cx);
        let restored = saved_layout.is_some();
        let restored_keys = saved_layout
            .as_ref()
            .map(|state| restored_panel_keys(&state.center))
            .unwrap_or_default();
        if let Some(state) = saved_layout {
            dock_area.update(cx, |area, cx| {
                area.load(state, window, cx)
                    .expect("saved dock layout must load");
            });
        }
        let open_panels = if restored {
            let ids = dock_area
                .read(cx)
                .layout(DockPlacement::Center)
                .map(|tree| tree.panels().collect::<Vec<_>>())
                .unwrap_or_default();
            // Paired by position - one key slot per restored panel - and a
            // panel with no key (an unrestored one) isn't tracked.
            ids.into_iter()
                .zip(restored_keys)
                .filter_map(|(id, key)| Some((id, key?)))
                .map(|(id, key)| OpenPanel {
                    id,
                    key,
                    panel: None,
                    group: crate::ui::panel::tabs::group_of(dock_area.read(cx), id),
                    _focus_watch: Self::watch_panel_focus(&dock_area, id, window, cx),
                })
                .collect()
        } else {
            vec![OpenPanel {
                key: PanelKey::from(&scope),
                id: first_id,
                // The first panel is the pods list, and the window did build
                // it, so the handle is available: keeping it lets a later
                // request for pods reuse the same `OpenedPanel` bookkeeping
                // every other panel gets.
                panel: Some(first.clone()),
                group: crate::ui::panel::tabs::group_of(dock_area.read(cx), first_id),
                _focus_watch: Self::watch_panel_focus(&dock_area, first_id, window, cx),
            }]
        };
        watch_workspace(&dock_area, window, cx);
        let resource_panel = cx.new(|cx| {
            crate::ui::resource_panel::ResourcePanel::new(
                context_name.clone(),
                contexts.clone(),
                window,
                cx,
            )
        });
        // 11.1: a new workspace opens its panel on the preferred edge.
        let resource_side = crate::ui::resource_panel::preferred_side(cx);
        resource_panel.update(cx, |panel, cx| panel.set_side(resource_side, cx));
        cx.subscribe_in(
            &resource_panel,
            window,
            |this: &mut MainWindow, _panel, event, window, cx| match event {
                crate::ui::resource_panel::ResourceEvent::Open(target) => {
                    this.open_target(target.clone(), window, cx);
                }
                crate::ui::resource_panel::ResourceEvent::SwitchContext(context_name) => {
                    this.set_active_context(context_name, window, cx);
                }
            },
        )
        .detach();
        let main_window_handle = cx.weak_entity();
        let status_bar = cx.new(|cx| {
            crate::ui::status_bar::StatusBarView::for_window(
                contexts.clone(),
                main_window_handle,
                cx,
            )
        });
        self.mode = WindowMode::Workspace {
            dock_area,
            _dock_skin: dock_skin,
            contexts,
            active: 0,
            resource_panel,
            open_panels,
            nav: Box::new(NavTarget::pods()),
            status_bar,
            resource_width: RESOURCE_PANEL_WIDTH,
            resource_side,
            resource_collapsed: false,
            last_focused_panel: None,
        };
        // Entering a workspace doesn't go through `sync_context_children` (its
        // children are built here already synced), so it titles the window itself.
        window_title::apply(&self.mode, window);
        if !restored {
            self.watch_scope_changes(first, window, cx);
        }
        cx.notify();
    }

    /// Re-files an open panel under the scope it now shows.
    ///
    /// A panel's title-bar namespace picker re-scopes the panel in place
    /// (10.2). Without this the window would keep the panel under its old key,
    /// so asking for that old scope again would focus a panel no longer showing
    /// it, and the panel's current scope would be unreachable.
    pub(super) fn watch_scope_changes(
        &mut self,
        opened: nav::OpenedPanel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // `PanelId` rather than the entity: the subscription closure has to be
        // `'static`, and an id says everything `rescope` needs.
        let id = opened.panel_id();
        let rekey = move |this: &mut MainWindow,
                          event: &panel_title::ScopeEvent,
                          cx: &mut Context<MainWindow>| {
            let panel_title::ScopeEvent::NamespacesChanged(namespaces) = event;
            this.rescope(id, namespaces.clone(), cx);
        };
        match opened {
            nav::OpenedPanel::Pods(panel) => {
                cx.subscribe_in(
                    &panel,
                    window,
                    move |this: &mut MainWindow, _panel, event, _window, cx| rekey(this, event, cx),
                )
                .detach();
            }
            nav::OpenedPanel::ObjectList(panel) => {
                cx.subscribe_in(
                    &panel,
                    window,
                    move |this: &mut MainWindow, _panel, event, _window, cx| rekey(this, event, cx),
                )
                .detach();
            }
            nav::OpenedPanel::Events(panel) => {
                cx.subscribe_in(
                    &panel,
                    window,
                    move |this: &mut MainWindow, _panel, event, _window, cx| rekey(this, event, cx),
                )
                .detach();
            }
            nav::OpenedPanel::Placeholder(panel) => {
                cx.subscribe_in(
                    &panel,
                    window,
                    move |this: &mut MainWindow, _panel, event, _window, cx| rekey(this, event, cx),
                )
                .detach();
            }
            nav::OpenedPanel::Logs(panel) => {
                cx.subscribe_in(
                    &panel,
                    window,
                    move |this: &mut MainWindow, _panel, event, _window, cx| rekey(this, event, cx),
                )
                .detach();
            }
            // A pod's detail panel shows one pod rather than a namespace-
            // filterable list of many, so it carries no picker and never
            // re-scopes.
            nav::OpenedPanel::PodDetail(_) | nav::OpenedPanel::ObjectDetail(_) => {}
        }
    }

    /// Moves the panel with `id` to a new namespace, whichever panel type it is.
    pub(super) fn rescope(&mut self, id: PanelId, namespaces: Vec<String>, cx: &mut Context<Self>) {
        let WindowMode::Workspace { open_panels, .. } = &mut self.mode else {
            return;
        };
        if let Some(open) = open_panels.iter_mut().find(|open| open.id == id) {
            open.key.namespaces = namespaces;
        }
        cx.notify();
    }

    /// Every context this window uses - empty in `Picker` mode. What `save`
    /// (`util/shell.rs`'s module doc comment) persists per window.
    pub(super) fn resource_width(&self) -> Option<Pixels> {
        match &self.mode {
            WindowMode::Picker(_) => None,
            WindowMode::Workspace { resource_width, .. } => Some(*resource_width),
        }
    }

    pub(super) fn set_resource_width(&mut self, width: Pixels) {
        if let WindowMode::Workspace { resource_width, .. } = &mut self.mode {
            *resource_width = width;
        }
    }

    pub(super) fn contexts(&self) -> Vec<String> {
        match &self.mode {
            WindowMode::Picker(_) => Vec::new(),
            WindowMode::Workspace { contexts, .. } => contexts.clone(),
        }
    }

    /// How many of this window's open panels use `context_name` - the count the
    /// Disconnect confirmation (`ui/status_bar/capsule.rs`) states before closing them.
    /// `0` in `Picker` mode, or for a context this window doesn't use.
    pub(crate) fn context_panel_count(&self, context_name: &str) -> usize {
        let WindowMode::Workspace { open_panels, .. } = &self.mode else {
            return 0;
        };
        open_panels
            .iter()
            .filter(|open| open.key.context_name == context_name)
            .count()
    }
}

#[cfg(test)]
mod tests;
