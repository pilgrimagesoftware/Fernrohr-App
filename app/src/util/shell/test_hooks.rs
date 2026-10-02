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
}
