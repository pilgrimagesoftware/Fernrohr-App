//! The harness picker Copy MCP Setup Command opens: the four harnesses, in
//! the Settings section's order. Up/Down and Enter, a filter, or a click
//! choose one; choosing copies its command and closes the picker. Escape
//! closes it and copies nothing. Hover moves only the list's own highlight,
//! never the row Enter acts on (`keyboard-first.md`).

use crate::mcp::setup::{AgentSetup, HARNESSES};
use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::command::{Command as CommandList, CommandItem, CommandState};
use gpui_kit::*;

/// The dialog's title.
pub(crate) const TITLE: &str = "Copy MCP Setup Command";

/// The debug selector of `harness`'s row.
pub(crate) fn row_selector(harness_id: &str) -> String {
    format!("agent-setup-harness {harness_id}")
}

/// Opens the picker in `window`.
pub(crate) fn open(window: &mut Window, cx: &mut App) {
    if !matches!(window.root::<Root>(), Some(Some(_))) {
        return;
    }
    let return_focus = window.focused(cx);
    let picker = cx.new(|cx| HarnessPicker {
        state: cx.new(|cx| CommandState::new(window, cx)),
        selected: Some(0),
        return_focus,
    });
    let state = picker.read(cx).state.clone();
    window.open_dialog(cx, move |dialog, _window, _cx| {
        let picker = picker.clone();
        dialog
            .title(TITLE)
            .content(move |content, _window, _cx| content.child(picker.clone()))
    });
    state.update(cx, |state, cx| state.focus(window, cx));
}

struct HarnessPicker {
    state: Entity<CommandState>,
    /// The row Enter acts on: the keyboard's, or the last clicked.
    selected: Option<usize>,
    return_focus: Option<FocusHandle>,
}

impl HarnessPicker {
    /// Copies harness `index`'s command and closes the picker.
    fn choose(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let AgentSetup::Ready { exe } = super::setup(cx) else {
            return;
        };
        let Some(harness) = HARNESSES.get(index) else {
            return;
        };
        crate::ui::copy::copy_text(&harness.command(&exe), cx);
        window.close_dialog(cx);
        if let Some(focus) = &self.return_focus {
            focus.focus(window, cx);
        }
    }
}

impl Render for HarnessPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let items: Vec<CommandItem> = HARNESSES
            .iter()
            .map(|harness| {
                let selector = row_selector(harness.id);
                CommandItem::new().label(harness.name).child(move |_, _| {
                    let selector = selector.clone();
                    div()
                        .flex_1()
                        .debug_selector(move || selector)
                        .child(harness.name)
                })
            })
            .collect();
        CommandList::new(&self.state)
            .items(items)
            .placeholder("Filter harnesses…")
            .on_select({
                let this = this.clone();
                move |index_path, window, cx| {
                    if !window.last_input_was_keyboard() {
                        return;
                    }
                    let _ = this.update(cx, |this, _| this.selected = Some(index_path.row));
                }
            })
            .on_confirm(move |index_path, window, cx| {
                let _ = this.update(cx, |this, cx| {
                    let index = if window.last_input_was_keyboard() {
                        this.selected.unwrap_or(index_path.row)
                    } else {
                        index_path.row
                    };
                    this.choose(index, window, cx);
                });
            })
    }
}
