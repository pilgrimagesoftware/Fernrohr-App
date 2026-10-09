//! Test-only constructors and accessors other modules' tests use to reach `MainWindow`'s private state.

use super::*;

impl MainWindow {
    /// Test-only: a bare `Picker`-mode window, for tests elsewhere in the crate
    /// that only need a real `WeakEntity<MainWindow>` to satisfy a constructor
    /// (`ui/status_bar.rs::StatusBarView::for_window`, which stores one but never reads
    /// it outside a click handler) - `mode` and `focus_handle` above have no
    /// visibility modifier, so nothing outside this module can build a
    /// `MainWindow` literal directly.
    #[cfg(test)]
    pub(crate) fn test_picker_window(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        }
    }

    /// Test-only: focuses the window's own handle, the way `focus_initial`
    /// does at launch, so a test's keystrokes have a focus path to dispatch on.
    #[cfg(test)]
    pub(crate) fn test_focus(&self, window: &mut Window, cx: &mut App) {
        self.focus_handle.focus(window, cx);
    }

    /// Test-only: a window already in `Workspace` mode on `contexts`, for tests
    /// that need a chip click or a Resource panel dropdown pick to actually reach
    /// [`Self::set_active_context`] and its downstream `sync_context_children`.
    /// Not `util/shell/tests.rs`'s own `connected_window`: that helper drives a
    /// real `ClusterConnection::connect`, which this one's callers don't need.
    /// Callers must pre-seed every context's `ClusterRegistry` session first
    /// (`insert_test_session`), so `enter_workspace`'s `hold` reuses it instead of
    /// starting a real connect.
    #[cfg(test)]
    pub(crate) fn test_workspace(
        contexts: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::test_picker_window(window, cx);
        this.enter_workspace(contexts, window, cx);
        this
    }

    /// Test-only readback of which context is active - `active` itself has no
    /// getter since production code only ever needs to write it (through
    /// [`Self::set_active_context`]).
    #[cfg(test)]
    pub(crate) fn test_active_context_name(&self) -> Option<String> {
        match &self.mode {
            WindowMode::Workspace {
                contexts, active, ..
            } => contexts.get(*active).cloned(),
            WindowMode::Picker(_) => None,
        }
    }

    /// Test-only access to the Resource panel, so a test can drive its cluster
    /// dropdown's `SwitchContext` exactly as a click does.
    #[cfg(test)]
    pub(crate) fn test_resource_panel(
        &self,
    ) -> Option<Entity<crate::ui::resource_panel::ResourcePanel>> {
        match &self.mode {
            WindowMode::Workspace { resource_panel, .. } => Some(resource_panel.clone()),
            WindowMode::Picker(_) => None,
        }
    }

    /// Test-only access to the window's status bar, so a test can assert the *real*
    /// bar [`Self::sync_context_children`] pushes into reflects a context change.
    #[cfg(test)]
    pub(crate) fn test_status_bar(&self) -> Option<Entity<crate::ui::status_bar::StatusBarView>> {
        match &self.mode {
            WindowMode::Workspace { status_bar, .. } => Some(status_bar.clone()),
            WindowMode::Picker(_) => None,
        }
    }

    /// Test-only: what each open panel shows, for tests outside the shell that
    /// check a key opened the panel it should.
    #[cfg(test)]
    pub(crate) fn test_open_targets(&self) -> Vec<crate::ui::nav::NavTarget> {
        match &self.mode {
            WindowMode::Workspace { open_panels, .. } => open_panels
                .iter()
                .map(|open| open.key.target.clone())
                .collect(),
            WindowMode::Picker(_) => Vec::new(),
        }
    }

    /// Test-only: the dock's own panel handle currently open for `target`, for
    /// a test that needs the concrete panel entity behind it (`saved-panel-
    /// layouts` 5.2's restored pod detail, whose own "not found" state lives
    /// behind its module boundary) rather than just the content key
    /// [`Self::test_open_targets`] reports.
    #[cfg(test)]
    pub(crate) fn test_panel_view_for(
        &self,
        target: &crate::ui::nav::NavTarget,
        cx: &App,
    ) -> Option<std::sync::Arc<dyn gpui_kit::base::dock::PanelView>> {
        let WindowMode::Workspace {
            open_panels,
            dock_area,
            ..
        } = &self.mode
        else {
            return None;
        };
        let id = open_panels
            .iter()
            .find(|open| &open.key.target == target)?
            .id;
        dock_area.read(cx).panel(id).cloned()
    }

    /// Test-only: every panel view currently in the centre dock, in tree
    /// order - for a test that needs to look past [`Self::test_open_targets`]'
    /// content keys (which a placeholder has none of) at what the dock
    /// actually built, e.g. `saved-panel-layouts` 5.1's missing-context
    /// placeholder or 5.3's unrecognised-kind one.
    /// Test-only: the dock's dump, as `layouts.save` captures it - for tests
    /// outside `util::shell` that need a real saved layout (`mcp::navigate`).
    #[cfg(test)]
    pub(crate) fn test_dock_dump(&self, cx: &App) -> gpui_kit::component::dock::DockAreaState {
        match &self.mode {
            WindowMode::Workspace { dock_area, .. } => dock_area.read(cx).dump(cx),
            WindowMode::Picker(_) => panic!("a dock dump needs a workspace window"),
        }
    }

    #[cfg(test)]
    pub(crate) fn test_dock_views(
        &self,
        cx: &App,
    ) -> Vec<std::sync::Arc<dyn gpui_kit::base::dock::PanelView>> {
        let WindowMode::Workspace { dock_area, .. } = &self.mode else {
            return Vec::new();
        };
        let Some(tree) = dock_area.read(cx).layout(DockPlacement::Center) else {
            return Vec::new();
        };
        tree.panels()
            .filter_map(|id| dock_area.read(cx).panel(id).cloned())
            .collect()
    }
}
