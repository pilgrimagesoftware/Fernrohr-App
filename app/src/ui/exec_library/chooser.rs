//! A dialog listing choices for one to be picked - which of an entry's matching
//! pods to run in, or which connected context (`exec-command-library` D5) -
//! with the first preselected, so Enter picks it. Filtering, the arrows and a
//! click reach every row; hover moves only the list's own highlight, never the
//! row Enter acts on. Escape closes the dialog and picks nothing.
//!
//! Picking closes the dialog, puts focus back where it was and then runs the
//! pick, so what it opens lands in the window the user was working in.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::command::{Command as CommandList, CommandItem, CommandState};
use gpui_kit::*;
use std::rc::Rc;

/// One row: what it is, and a muted detail beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Choice {
    pub(crate) label: String,
    pub(crate) detail: Option<String>,
}

/// The debug selector of row `index`.
pub(crate) fn row_selector(index: usize) -> String {
    format!("exec-chooser-row {index}")
}

type OnPick = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// Opens a chooser titled `title` over `choices`; `on_pick` gets the picked
/// row's index.
pub(crate) fn open(
    title: impl Into<SharedString>,
    choices: Vec<Choice>,
    on_pick: impl Fn(usize, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    if !matches!(window.root::<Root>(), Some(Some(_))) {
        return;
    }
    let return_focus = window.focused(cx);
    let chooser = cx.new(|cx| Chooser {
        choices,
        state: cx.new(|cx| CommandState::new(window, cx)),
        selected: Some(0),
        return_focus,
        on_pick: Rc::new(on_pick),
    });
    let state = chooser.read(cx).state.clone();
    let title = title.into();
    window.open_dialog(cx, move |dialog, _window, _cx| {
        let chooser = chooser.clone();
        dialog
            .title(title.clone())
            .content(move |content, _window, _cx| content.child(chooser.clone()))
    });
    state.update(cx, |state, cx| state.focus(window, cx));
}

struct Chooser {
    choices: Vec<Choice>,
    state: Entity<CommandState>,
    /// The row Enter acts on: the keyboard's, or the last clicked.
    selected: Option<usize>,
    return_focus: Option<FocusHandle>,
    on_pick: OnPick,
}

impl Chooser {
    fn pick(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.choices.len() {
            return;
        }
        window.close_dialog(cx);
        if let Some(focus) = &self.return_focus {
            focus.focus(window, cx);
        }
        let on_pick = self.on_pick.clone();
        window.defer(cx, move |window, cx| on_pick(index, window, cx));
    }
}

impl Render for Chooser {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let muted = cx.theme().muted_foreground;
        let items: Vec<CommandItem> =
            self.choices
                .iter()
                .enumerate()
                .map(|(index, choice)| {
                    let choice = choice.clone();
                    CommandItem::new()
                        .label(choice.label.clone())
                        .child(move |_, _| {
                            div()
                                .flex_1()
                                .flex()
                                .items_center()
                                .gap_2()
                                .debug_selector(move || row_selector(index))
                                .child(choice.label.clone())
                                .children(
                                    choice.detail.clone().map(|detail| {
                                        div().text_sm().text_color(muted).child(detail)
                                    }),
                                )
                        })
                })
                .collect();
        CommandList::new(&self.state)
            .items(items)
            .placeholder("Filter…")
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
                    this.pick(index, window, cx);
                });
            })
    }
}
