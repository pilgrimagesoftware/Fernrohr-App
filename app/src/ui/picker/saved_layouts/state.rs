//! `SavedLayoutsPicker`'s construction and core state: the saved layouts and
//! any unreadable files (read via [`saved_layouts::load_all`]), the selection
//! the keyboard and mouse share, and the inline rename field. Click/keyboard
//! selection and the rename/delete commands live in [`super::interaction`];
//! the view's own `Render` wiring lives in [`super::render`].

use super::*;

/// The inline rename field a row shows in place of its name while it is
/// open, and any collision error it reported - `super::interaction` owns
/// starting, committing and cancelling it.
pub(super) struct RenameState {
    pub(super) index: usize,
    pub(super) input: Entity<InputState>,
    pub(super) error: Option<SharedString>,
    /// Clears a stale error the moment the field's text changes again. Held
    /// only so dropping the `RenameState` cancels it.
    pub(super) _subscription: Subscription,
}

pub struct SavedLayoutsPicker {
    pub(super) dir: PathBuf,
    pub(super) layouts: Vec<SavedLayout>,
    pub(super) unreadable: Vec<UnreadableLayout>,
    pub(super) command_state: Entity<CommandState>,
    pub(super) selected_index: Option<usize>,
    pub(super) rename: Option<RenameState>,
    /// The seam section 4 (`saved_layouts.load_replace`/`load_add`) acts
    /// through: the window this picker opened over, so loading the selected
    /// layout needs no new plumbing once those commands exist.
    // UNWIRED(#176): nothing reads this or `Self::main_window` yet - section
    // 4's `load_replace`/`load_add` commands are the first real callers. A
    // test that only constructs a `SavedLayoutsPicker` is not coverage of
    // that later section.
    #[allow(dead_code)]
    main_window: WeakEntity<MainWindow>,
}

impl SavedLayoutsPicker {
    pub fn new(
        dir: PathBuf,
        main_window: WeakEntity<MainWindow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (layouts, unreadable) = saved_layouts::load_all(&dir);
        let selected_index = (!layouts.is_empty()).then_some(0);
        Self {
            dir,
            layouts,
            unreadable,
            command_state: cx.new(|cx| CommandState::new(window, cx)),
            selected_index,
            rename: None,
            main_window,
        }
    }

    /// Re-reads [`Self::dir`] after a rename or a delete, clamping the
    /// selection so it still names a row - or `None`, if the list just
    /// emptied.
    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        let (layouts, unreadable) = saved_layouts::load_all(&self.dir);
        self.layouts = layouts;
        self.unreadable = unreadable;
        self.selected_index = match self.selected_index {
            Some(index) if index < self.layouts.len() => Some(index),
            _ if self.layouts.is_empty() => None,
            _ => Some(self.layouts.len() - 1),
        };
        cx.notify();
    }

    /// The seam section 4 acts on: the layout the keyboard or mouse currently
    /// highlights, if any.
    pub(crate) fn selected(&self) -> Option<&SavedLayout> {
        self.selected_index
            .and_then(|index| self.layouts.get(index))
    }

    /// The seam section 4 acts through: the window this picker should load
    /// Add/Replace into.
    // UNWIRED(#176): see the field's own doc comment above.
    #[allow(dead_code)]
    pub(crate) fn main_window(&self) -> WeakEntity<MainWindow> {
        self.main_window.clone()
    }

    /// Where keyboard focus belongs when the picker opens: its list - always
    /// drawn, even with nothing in it (`Command`'s own `.empty()` slot draws
    /// the "no saved layouts" message in its place), so there is never a
    /// fallback case the way `ClusterPicker` needs one for an unreadable
    /// kubeconfig.
    pub(crate) fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.command_state.read(cx).focus_handle(cx)
    }

    /// Test-only: every listed layout's name, in the order the picker shows
    /// them.
    #[cfg(test)]
    pub(crate) fn test_layout_names(&self) -> Vec<String> {
        self.layouts
            .iter()
            .map(|layout| layout.name.clone())
            .collect()
    }

    /// Test-only: the layout the keyboard or mouse currently highlights.
    #[cfg(test)]
    pub(crate) fn test_selected_name(&self) -> Option<String> {
        self.selected().map(|layout| layout.name.clone())
    }
}

#[cfg(test)]
mod tests;
