//! Click and keyboard selection: turning a row click ([`ClusterPicker::handle_row_click`]),
//! `Enter` ([`ClusterPicker::confirm_row`]), the Connect button
//! ([`ClusterPicker::connect_selected`]), or keyboard navigation
//! ([`ClusterPicker::follow_keyboard`]) into the picker's one selection model and, where
//! the action warrants it, a [`ClusterPicker::select`] (defined in [`super::state`]) call.

use super::*;

impl ClusterPicker {
    /// What a row's own click handler ([`context_row`]) calls, replacing `Command`'s
    /// built-in "any click confirms" behavior for this list: a single click only
    /// moves the highlight (mirroring hover), so an inadvertent click no longer
    /// starts a connection. A double click still connects immediately, matching
    /// familiar file-manager conventions.
    pub(crate) fn handle_row_click(
        &mut self,
        context_name: String,
        row_index: usize,
        click_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if click_count >= 2 {
            self.select(context_name, cx);
            return;
        }
        self.selected_context = Some(context_name);
        self.command_state.update(cx, |state, cx| {
            state.set_selected_index(Some(IndexPath::new(row_index)), window, cx)
        });
        cx.notify();
    }

    /// What `Enter` reaches through `Command`'s own confirm action and the
    /// `on_confirm` wired in `render`: connects the context at `row_index` in the
    /// picker's own model order - the same coordinates `Command::on_select` and
    /// `Command::on_confirm` report, unaffected by the search query (see their doc
    /// comments).
    pub(crate) fn confirm_row(&mut self, row_index: usize, cx: &mut Context<Self>) {
        let Ok(contexts) = &self.contexts else {
            return;
        };
        let Some(context_name) = contexts.get(row_index).cloned() else {
            return;
        };
        self.select(context_name, cx);
    }

    /// The context the user selected - by click or keyboard - if any.
    pub(crate) fn selected_context(&self) -> Option<String> {
        self.selected_context.clone()
    }

    /// Keyboard navigation moved `Command`'s highlight to `row_index`: select it.
    pub(super) fn follow_keyboard(&mut self, row_index: usize, cx: &mut Context<Self>) {
        self.selected_context = self
            .contexts
            .as_ref()
            .ok()
            .and_then(|contexts| contexts.get(row_index).cloned());
        cx.notify();
    }

    /// What [`Self::connect_button`]'s click handler calls: connects the currently
    /// highlighted context, or does nothing if none is highlighted. The button
    /// itself is disabled in that case, but a click that slips through must still
    /// be a no-op rather than connecting the wrong thing.
    pub(crate) fn connect_selected(&mut self, cx: &mut Context<Self>) {
        let Some(context_name) = self.selected_context.clone() else {
            return;
        };
        self.select(context_name, cx);
    }
}

#[cfg(test)]
mod tests;
