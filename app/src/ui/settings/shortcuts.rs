//! The Settings window's Keyboard Shortcuts section: its state and actions.
//! Drawing it lives in [`render`]; the rows' content in [`super::rows`].
//!
//! The list has its own key context, [`CONTEXT`], and the filter field sits
//! outside it, so the section's keys (Backspace = Remove Shortcut) never fire
//! while typing a filter.

use super::recorder::{Recorded, Recorder};
use super::rows::{self, Row};
use crate::command::{Command, CommandRegistry};
use crate::keymap::{self, Edit, LiveKeymap};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;

mod render;

actions!(
    settings_shortcuts,
    [
        RecordShortcut,
        ResetShortcut,
        RemoveShortcut,
        FilterShortcuts,
        SelectNextShortcut,
        SelectPreviousShortcut
    ]
);

/// The shortcut list's key context.
pub const CONTEXT: &str = "KeyboardShortcuts";

/// The section's commands, in [`CONTEXT`]: every editor action is a registered
/// command with a key `keymap.toml` - or this very editor - can change.
pub fn register_commands(registry: &mut CommandRegistry) {
    for (id, title, default_binding, action) in [
        (
            "settings.shortcuts.record",
            "Record Shortcut",
            "enter",
            Box::new(RecordShortcut) as Box<dyn Action>,
        ),
        (
            "settings.shortcuts.reset",
            "Reset Shortcut to Default",
            "cmd-backspace",
            Box::new(ResetShortcut),
        ),
        (
            "settings.shortcuts.remove",
            "Remove Shortcut",
            "backspace",
            Box::new(RemoveShortcut),
        ),
        (
            "settings.shortcuts.filter",
            "Filter Shortcuts",
            "/",
            Box::new(FilterShortcuts),
        ),
    ] {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: Some(CONTEXT),
            action,
            menu: None,
        });
    }
}

/// ↑/↓ between rows: pure cursor movement, the one kind of key the
/// keyboard-first rule leaves as a raw binding.
pub fn list_bindings() -> [KeyBinding; 2] {
    [
        KeyBinding::new("down", SelectNextShortcut, Some(CONTEXT)),
        KeyBinding::new("up", SelectPreviousShortcut, Some(CONTEXT)),
    ]
}

/// Where the section is in an edit.
enum Mode {
    Browsing,
    /// Waiting for the new key for `id`.
    Recording {
        id: &'static str,
        _recorder: Recorder,
    },
    /// `keys` is already used by `others` in the same scope; Enter applies,
    /// Escape cancels.
    Confirming {
        id: &'static str,
        keys: String,
        others: Vec<&'static str>,
        _recorder: Recorder,
    },
}

pub struct ShortcutsSection {
    filter: Entity<InputState>,
    list_focus: FocusHandle,
    scroll: ScrollHandle,
    /// The selected row, by command id - stable across filtering.
    selected: Option<&'static str>,
    mode: Mode,
    /// The last save failure, shown until the next edit.
    error: Option<String>,
    _filter_changes: Subscription,
}

impl ShortcutsSection {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter shortcuts…"));
        let list_focus = cx.focus_handle();
        let _filter_changes = cx.subscribe_in(&filter, window, {
            let list_focus = list_focus.clone();
            move |this: &mut Self, _input, event, window, cx| match event {
                InputEvent::Change => {
                    this.keep_selection_visible(cx);
                    cx.notify();
                }
                // Enter in the filter goes back to the list, to act on a match.
                InputEvent::PressEnter { .. } => window.focus(&list_focus, cx),
                _ => {}
            }
        });
        Self {
            filter,
            list_focus,
            scroll: ScrollHandle::new(),
            selected: None,
            mode: Mode::Browsing,
            error: None,
            _filter_changes,
        }
    }

    pub fn list_focus(&self) -> FocusHandle {
        self.list_focus.clone()
    }

    /// Test-only: the selected row's id.
    #[cfg(test)]
    pub(super) fn test_selected(&self, cx: &App) -> Option<&'static str> {
        self.selected_id(cx)
    }

    /// Test-only: `"browsing"`, `"recording"` or `"confirming"`.
    #[cfg(test)]
    pub(super) fn test_mode(&self) -> &'static str {
        match self.mode {
            Mode::Browsing => "browsing",
            Mode::Recording { .. } => "recording",
            Mode::Confirming { .. } => "confirming",
        }
    }

    /// Test-only: the filter field's focus handle.
    #[cfg(test)]
    pub(super) fn test_filter_focus(&self, cx: &App) -> FocusHandle {
        self.filter.read(cx).focus_handle(cx)
    }

    /// The rows the filter leaves, from the live keymap.
    pub(super) fn visible_rows(&self, cx: &App) -> Vec<Row> {
        let (Some(registry), Some(live)) = (
            cx.try_global::<CommandRegistry>(),
            cx.try_global::<LiveKeymap>(),
        ) else {
            return Vec::new();
        };
        let query = self.filter.read(cx).value().to_string();
        rows::rows(registry, live.config())
            .into_iter()
            .filter(|row| rows::matches(row, &query))
            .collect()
    }

    /// The selected row if the filter still shows it, else the first shown.
    fn current(&self, rows: &[Row]) -> Option<usize> {
        self.selected
            .and_then(|id| rows.iter().position(|row| row.id == id))
            .or((!rows.is_empty()).then_some(0))
    }

    fn keep_selection_visible(&mut self, cx: &App) {
        let rows = self.visible_rows(cx);
        if let Some(ix) = self.current(&rows) {
            self.selected = Some(rows[ix].id);
            self.scroll.scroll_to_item(ix);
        }
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let rows = self.visible_rows(cx);
        let Some(ix) = self.current(&rows) else {
            return;
        };
        let next = ix.saturating_add_signed(delta).min(rows.len() - 1);
        self.selected = Some(rows[next].id);
        self.scroll.scroll_to_item(next);
        cx.notify();
    }

    pub(super) fn select(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.selected = Some(id);
        cx.notify();
    }

    fn selected_id(&self, cx: &App) -> Option<&'static str> {
        let rows = self.visible_rows(cx);
        self.current(&rows).map(|ix| rows[ix].id)
    }

    /// Starts recording a new key for the selected row.
    pub(super) fn record(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected_id(cx) else {
            return;
        };
        self.selected = Some(id);
        let this = cx.weak_entity();
        let recorder = Recorder::start(window, cx, move |recorded, window, cx| {
            let _ = this.update(cx, |this, cx| this.on_recorded(recorded, window, cx));
        });
        self.mode = Mode::Recording {
            id,
            _recorder: recorder,
        };
        self.error = None;
        cx.notify();
    }

    fn on_recorded(&mut self, recorded: Recorded, window: &mut Window, cx: &mut Context<Self>) {
        match (&self.mode, recorded) {
            (_, Recorded::Cancelled) => self.stop(cx),
            (Mode::Recording { id, .. }, Recorded::Keys(keys)) => {
                let id = *id;
                let others = cx
                    .try_global::<LiveKeymap>()
                    .map(|live| {
                        keymap::conflicts(cx.global::<CommandRegistry>(), live.config(), id, &keys)
                            .same_scope
                    })
                    .unwrap_or_default();
                if others.is_empty() {
                    self.stop(cx);
                    self.edit(id, Edit::Set(keys), cx);
                } else {
                    // Keep intercepting: Enter and Escape answer the prompt.
                    let Mode::Recording { _recorder, .. } =
                        std::mem::replace(&mut self.mode, Mode::Browsing)
                    else {
                        unreachable!("matched Recording above")
                    };
                    self.mode = Mode::Confirming {
                        id,
                        keys,
                        others,
                        _recorder,
                    };
                    cx.notify();
                }
            }
            (Mode::Confirming { .. }, Recorded::Keys(keys)) if keys == "enter" => {
                self.confirm(cx);
            }
            _ => {}
        }
        let _ = window;
    }

    /// Applies the key waiting on the conflict prompt.
    pub(super) fn confirm(&mut self, cx: &mut Context<Self>) {
        if let Mode::Confirming { id, keys, .. } = std::mem::replace(&mut self.mode, Mode::Browsing)
        {
            self.edit(id, Edit::Set(keys), cx);
        }
        cx.notify();
    }

    /// Stops recording or prompting, changing nothing.
    pub(super) fn stop(&mut self, cx: &mut Context<Self>) {
        self.mode = Mode::Browsing;
        cx.notify();
    }

    fn edit(&mut self, id: &'static str, edit: Edit, cx: &mut Context<Self>) {
        match keymap::apply(cx, id, edit) {
            Ok(()) => {
                self.error = None;
                crate::ui::menu::rebuild_menus(cx);
            }
            Err(error) => self.error = Some(format!("Couldn't save keymap.toml: {error}")),
        }
        cx.notify();
    }

    pub(super) fn reset(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_id(cx) {
            self.edit(id, Edit::Reset, cx);
        }
    }

    pub(super) fn remove(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_id(cx) {
            self.edit(id, Edit::Remove, cx);
        }
    }

    fn on_action_record(
        &mut self,
        _: &RecordShortcut,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.record(window, cx);
    }

    fn on_action_reset(&mut self, _: &ResetShortcut, _: &mut Window, cx: &mut Context<Self>) {
        self.reset(cx);
    }

    fn on_action_remove(&mut self, _: &RemoveShortcut, _: &mut Window, cx: &mut Context<Self>) {
        self.remove(cx);
    }

    fn on_action_filter(
        &mut self,
        _: &FilterShortcuts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let handle = self.filter.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    }

    fn on_action_next(&mut self, _: &SelectNextShortcut, _: &mut Window, cx: &mut Context<Self>) {
        self.step(1, cx);
    }

    fn on_action_previous(
        &mut self,
        _: &SelectPreviousShortcut,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step(-1, cx);
    }
}

#[cfg(test)]
mod tests;
