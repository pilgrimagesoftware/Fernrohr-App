//! Click/keyboard selection ([`SavedLayoutsPicker::follow_keyboard`],
//! [`SavedLayoutsPicker::handle_row_click`], mirroring `ClusterPicker`'s own)
//! and the picker's four commands: `saved_layouts.rename_selected` opens an
//! inline rename field seeded with the selected layout's name (Enter
//! commits, Escape cancels, a collision shows [`RenameError::NameTaken`]
//! inline without losing either name); `saved_layouts.delete_selected` asks
//! through the app's Irreversible confirmation (design.md D6) before
//! removing the file; `saved_layouts.load_replace`/`load_add`
//! ([`SavedLayoutsPicker::load_selected`]) close this dialog and apply the
//! selected layout to the window this picker opened over.

use super::*;
use gpui_kit::component::WindowExt as _;

impl SavedLayoutsPicker {
    /// Keyboard navigation moved `Command`'s highlight to `row_index`:
    /// select it - mirrors `ClusterPicker::follow_keyboard`, so hover (routed
    /// through `super::render`'s own `on_select` guard) never reaches this.
    pub(super) fn follow_keyboard(&mut self, row_index: usize, cx: &mut Context<Self>) {
        self.selected_index = Some(row_index);
        cx.notify();
    }

    /// A row's own click handler (see `render`'s row builder): a click always
    /// just selects, the same as a keyboard move - there is no confirm-on-
    /// click action yet (section 4 adds Add/Replace).
    pub(super) fn handle_row_click(
        &mut self,
        row_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_index = Some(row_index);
        self.command_state.update(cx, |state, cx| {
            state.set_selected_index(Some(IndexPath::new(row_index)), window, cx)
        });
        cx.notify();
    }

    /// `saved_layouts.rename_selected` (`r`): opens an inline rename field
    /// seeded with the selected row's current name. A no-op with no
    /// selection, or while already renaming another row.
    pub(super) fn on_action_rename_selected(
        &mut self,
        _: &RenameSelected,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.rename.is_some() {
            return;
        }
        let Some(index) = self.selected_index else {
            return;
        };
        let Some(layout) = self.layouts.get(index) else {
            return;
        };
        self.start_rename(index, layout.name.clone(), window, cx);
    }

    /// Opens the inline rename field for row `index`. A row's own pencil
    /// button (`render`) calls this directly - the same seam the `r` command
    /// uses, so renaming has both a keyboard and a mouse route
    /// (`keyboard-first.md`).
    pub(super) fn start_rename(
        &mut self,
        index: usize,
        current_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| InputState::new(window, cx).default_value(current_name));
        // Selected, not just seeded: typing immediately replaces the current
        // name rather than inserting at a cursor sitting in front of it - the
        // usual "rename" interaction (Finder, VS Code), and what lets a bare
        // `r`, type, Enter sequence land on exactly the typed name.
        input.update(cx, |state, cx| state.select_all(window, cx));
        let subscription =
            cx.subscribe_in(&input, window, |this: &mut Self, _, event, _window, cx| {
                if let InputEvent::Change = event {
                    if let Some(rename) = &mut this.rename {
                        rename.error = None;
                    }
                    cx.notify();
                }
            });
        let focus = input.read(cx).focus_handle(cx);
        self.rename = Some(RenameState {
            index,
            input,
            error: None,
            _subscription: subscription,
        });
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Enter in the rename field: commits through
    /// [`saved_layouts::rename`], or shows [`RenameError::NameTaken`] inline
    /// without losing what was typed (spec "Renaming to a name already in
    /// use"). An unchanged or blank name just closes the field - neither is
    /// a rename.
    pub(super) fn commit_rename(&mut self, cx: &mut Context<Self>) {
        let Some(rename) = &self.rename else {
            return;
        };
        let Some(old_name) = self
            .layouts
            .get(rename.index)
            .map(|layout| layout.name.clone())
        else {
            self.rename = None;
            cx.notify();
            return;
        };
        let new_name = rename.input.read(cx).value().trim().to_string();
        if new_name.is_empty() || new_name == old_name {
            self.rename = None;
            cx.notify();
            return;
        }
        match saved_layouts::rename(&self.dir, &old_name, &new_name) {
            Ok(()) => {
                self.rename = None;
                self.reload(cx);
            }
            Err(RenameError::NameTaken) => {
                if let Some(rename) = &mut self.rename {
                    rename.error = Some("Another saved layout already has that name.".into());
                }
                cx.notify();
            }
            Err(error) => {
                if let Some(rename) = &mut self.rename {
                    rename.error = Some(format!("Couldn't rename this layout: {error}").into());
                }
                cx.notify();
            }
        }
    }

    /// Escape in the rename field: discards it, both names unchanged.
    pub(super) fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.rename = None;
        cx.notify();
    }

    /// `saved_layouts.delete_selected` (`backspace`): asks through the app's
    /// Irreversible confirmation (design.md D6) before removing the file,
    /// then reloads the list. A no-op with no selection.
    pub(super) fn on_action_delete_selected(
        &mut self,
        _: &DeleteSelected,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(layout) = self.selected().cloned() else {
            return;
        };
        let dir = self.dir.clone();
        let this = cx.weak_entity();
        let confirmation = Confirmation {
            title: "Delete Saved Layout?".into(),
            body: ConfirmText::new()
                .text("Deleting ")
                .name(&layout.name)
                .text(" can't be undone."),
            confirm: "Delete".into(),
            id_prefix: DELETE_ID_PREFIX,
            severity: Severity::Irreversible,
        };
        confirm_dialog::open(
            confirmation,
            move |_window, cx| {
                if let Err(error) = saved_layouts::remove(&dir, &layout.name) {
                    log::warn!("failed to remove saved layout {:?}: {error}", layout.name);
                }
                let _ = this.update(cx, |this, cx| this.reload(cx));
            },
            window,
            cx,
        );
    }

    /// `saved_layouts.load_replace` (`enter`): acts on the selected layout
    /// with Replace. The actual `enter` *keystroke* never dispatches this
    /// `LoadReplace` action - `gpui_component`'s own `Command` widget binds
    /// `enter` to its built-in `Confirm` in its own `"Command"` key context,
    /// which sits deeper in the render tree than this picker's own and so
    /// wins first (`render`'s `Command::on_confirm` calls
    /// [`Self::confirm_selected`], the same method below, for that path) -
    /// this handler exists so `LoadReplace` still does the right thing
    /// wherever else it's dispatched (the command palette, or a direct
    /// `window.dispatch_action` in a test), without a second copy of the
    /// loading logic.
    pub(super) fn on_action_load_replace(
        &mut self,
        _: &LoadReplace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.confirm_selected(window, cx);
    }

    /// `saved_layouts.load_add` (`secondary-enter`): acts on the selected
    /// layout with Add. Unlike Replace's `enter`, `secondary-enter` is not
    /// one of `Command`'s own built-in bindings, so this `on_action` is the
    /// only route a keystroke reaches it by - no `on_confirm` detour needed.
    pub(super) fn on_action_load_add(
        &mut self,
        _: &LoadAdd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.load_selected(LoadMode::Add, window, cx);
    }

    /// What `render`'s `Command::on_confirm` reaches for a bare `Enter` -
    /// Replace, the picker's "I asked for *that* layout" action (design.md
    /// D4). Calls the same [`Self::load_selected`] `on_action_load_replace`
    /// does, so Enter does Replace exactly once no matter which of the two
    /// paths dispatches it.
    pub(super) fn confirm_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.load_selected(LoadMode::Replace, window, cx);
    }

    /// Closes the picker dialog and applies the selected layout to the
    /// window this picker opened over - a no-op with nothing selected, or if
    /// that window is already gone (closed or disconnected while the picker
    /// was open).
    fn load_selected(&mut self, mode: LoadMode, window: &mut Window, cx: &mut Context<Self>) {
        let Some(layout) = self.selected().cloned() else {
            return;
        };
        let Some(main_window) = self.main_window().upgrade() else {
            return;
        };
        window.close_dialog(cx);
        main_window.update(cx, |main_window, cx| match mode {
            LoadMode::Replace => main_window.load_replace(layout, window, cx),
            LoadMode::Add => main_window.load_add(layout, window, cx),
        });
    }
}

/// Which of [`SavedLayoutsPicker::load_selected`]'s two loading actions to
/// carry out.
enum LoadMode {
    Replace,
    Add,
}

#[cfg(test)]
mod tests;
