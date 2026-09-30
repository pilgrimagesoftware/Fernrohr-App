//! The Resource panel's keyboard action handlers, and the one action of its
//! four (`/`) that goes through the app-wide command registry rather than a
//! raw `KeyBinding`.
//!
//! `/` needs a palette entry and a `keymap.toml` override the same as any
//! other command, so it is registered as a [`crate::command::Command`] with
//! `context: Some(PANEL_KEY_CONTEXT)` - `keymap::bindings` then binds it
//! automatically (see `util/shell.rs::init`), the same as `nav.show_pods` and
//! every other registered command. Up/Down/Enter/Left/Right have no palette
//! or menu presence, so they stay raw bindings in [`super::keyboard`] instead
//! (the same split `pods::panel_bindings` documents).

use super::keyboard::{
    CollapseSection, ExpandSection, OpenSelected, PANEL_KEY_CONTEXT, SelectNext, SelectPrevious,
};
use super::{ResourcePanel, keyboard};
use crate::command::{Command, CommandRegistry};
use crate::ui::nav::NavTarget;
use gpui_kit::component::input::Escape;
use gpui_kit::{Context, Focusable as _, Window, actions};

actions!(resource_panel, [FocusFilter]);

pub(super) const FOCUS_FILTER_COMMAND_ID: &str = "resource.focus_filter";
pub(super) const FOCUS_FILTER_DEFAULT_BINDING: &str = "/";

/// The commands this module contributes to the app-wide [`CommandRegistry`] -
/// see this module's doc comment for why `/` alone goes through it.
///
/// `pub(crate)`: `util/shell.rs::register_commands` calls this across the
/// module boundary the same way it calls `nav::register_commands` and
/// `tunnels::register_commands`.
pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: FOCUS_FILTER_COMMAND_ID,
        title: "Focus Resource Filter",
        default_binding: FOCUS_FILTER_DEFAULT_BINDING,
        context: Some(PANEL_KEY_CONTEXT),
        action: Box::new(FocusFilter),
        menu: None,
    });
}

impl ResourcePanel {
    /// Down: the next visible row after `highlighted`, or the first row when
    /// nothing is highlighted yet. Never wraps past the last row.
    pub(super) fn on_action_select_next(
        &mut self,
        _: &SelectNext,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let sections = self.visible_sections(cx);
        let visible = keyboard::visible_kinds(&sections);
        let current = self.highlighted_kind();
        if let Some(next) = keyboard::next(&visible, current.as_ref()) {
            self.set_highlighted(Some(NavTarget::Kind(next)), cx);
        }
    }

    /// Up: the mirror of `on_action_select_next`.
    pub(super) fn on_action_select_previous(
        &mut self,
        _: &SelectPrevious,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let sections = self.visible_sections(cx);
        let visible = keyboard::visible_kinds(&sections);
        let current = self.highlighted_kind();
        if let Some(previous) = keyboard::previous(&visible, current.as_ref()) {
            self.set_highlighted(Some(NavTarget::Kind(previous)), cx);
        }
    }

    /// Enter: opens the highlighted row exactly as a double-click does - both
    /// call [`ResourcePanel::request_open`]. A no-op with nothing highlighted.
    pub(super) fn on_action_open_selected(
        &mut self,
        _: &OpenSelected,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(target) = self.highlighted.clone() {
            self.request_open(target, cx);
        }
    }

    /// Left: collapses the highlighted row's own section. A no-op with
    /// nothing highlighted - there is no "selected row's section" to collapse.
    pub(super) fn on_action_collapse_section(
        &mut self,
        _: &CollapseSection,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(category) = self.highlighted_category() {
            self.collapsed.insert(category);
            cx.notify();
        }
    }

    /// Right: the mirror of `on_action_collapse_section`.
    pub(super) fn on_action_expand_section(
        &mut self,
        _: &ExpandSection,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(category) = self.highlighted_category() {
            self.collapsed.remove(&category);
            cx.notify();
        }
    }

    /// `/`: focuses the filter box. Registered as a `Command` rather than a
    /// raw binding - see this module's doc comment.
    pub(super) fn on_action_focus_filter(
        &mut self,
        _: &FocusFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let handle = self.filter_input.read(cx).focus_handle(cx);
        handle.focus(window, cx);
    }

    /// Escape, while the filter has focus: clears it and returns focus to the
    /// list. `Escape` is `gpui_kit`'s own input action, already bound to
    /// "escape" within `InputState`'s "Input" key context - this panel just
    /// intercepts it on the way up the focus path, the same pattern
    /// gpui-component's own `input::search::SearchPanel` uses for the same
    /// action.
    pub(super) fn on_action_clear_filter(
        &mut self,
        _: &Escape,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.filter_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.focus_handle.focus(window, cx);
        cx.notify();
    }
}
