//! The dialog that lists the saved sets, in file order, for a set command to
//! act on: switch the focused list to one, apply one to its context, edit
//! one, or delete one.
//!
//! In the two switch modes, with a namespaced list in focus, the first nine
//! sets carry the digit `1`-`9`, and typing a digit applies that set - two
//! keystrokes from the panel - rather than typing it into the filter field.
//! Every set is also reachable by filtering, the arrows and Enter, or a click. Hover moves only the list's own highlight, never the
//! row Enter acts on (the `links::go_to` rule). Escape closes the dialog and
//! changes nothing. Deleting asks first, naming the set.
//!
//! Applying closes the dialog, puts focus back on the panel it was opened
//! from and dispatches [`ApplyNamespaceSet`] from there, so it reaches that
//! panel's window from inside its workspace rather than from the dialog layer.

use super::store::NamespaceSets;
use super::{ApplyNamespaceSet, NEEDS_NAMESPACED_PANEL};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::command::{Command as CommandList, CommandItem, CommandState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// What the picked set is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// Switch the focused list to it.
    Switch,
    /// Apply it to every namespaced list in the focused panel's context.
    SwitchContext,
    /// Edit it.
    Edit,
    /// Delete it, once confirmed.
    Remove,
}

impl Purpose {
    fn title(self) -> &'static str {
        match self {
            Purpose::Switch => "Switch Namespace Set",
            Purpose::SwitchContext => "Apply Namespace Set to Context",
            Purpose::Edit => "Edit Namespace Set",
            Purpose::Remove => "Remove Namespace Set",
        }
    }

    fn switches(self) -> bool {
        match self {
            Purpose::Switch | Purpose::SwitchContext => true,
            Purpose::Edit | Purpose::Remove => false,
        }
    }
}

/// How many sets carry a digit.
const DIGITS: usize = 9;

/// The debug selector of set `name`'s row.
pub fn row_selector(name: &str) -> String {
    format!("namespace-set-row {name}")
}

/// The debug selector of the digit beside row `index`.
pub fn digit_selector(index: usize) -> String {
    format!("namespace-set-digit {index}")
}

/// The debug selector of the dialog's notice line.
pub const NOTICE_SELECTOR: &str = "namespace-set-notice";

/// The delete confirmation's buttons.
pub const CONFIRM_DELETE_ID: &str = "namespace-set-confirm-delete";
pub const CANCEL_DELETE_ID: &str = "namespace-set-cancel-delete";

/// Opens the picker for `purpose`. `has_target` is whether a namespaced list
/// had focus when the command ran - what the switch modes act on. Edit
/// hands the chosen set to `on_edit`.
pub fn open(
    purpose: Purpose,
    has_target: bool,
    on_edit: impl Fn(String, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    if !matches!(window.root::<Root>(), Some(Some(_))) {
        return;
    }
    let return_focus = window.focused(cx);
    let picker = cx.new(|cx| SetPicker {
        purpose,
        has_target,
        return_focus,
        state: cx.new(|cx| CommandState::new(window, cx)),
        selected: Some(0),
        notice: None,
        confirming: None,
        on_edit: std::rc::Rc::new(on_edit),
        focus_handle: cx.focus_handle(),
    });
    let state = picker.read(cx).state.clone();
    window.open_dialog(cx, move |dialog, _window, _cx| {
        let picker = picker.clone();
        dialog
            .title(purpose.title())
            .content(move |content, _window, _cx| content.child(picker.clone()))
    });
    state.update(cx, |state, cx| state.focus(window, cx));
}

type OnEdit = std::rc::Rc<dyn Fn(String, &mut Window, &mut App)>;

pub struct SetPicker {
    purpose: Purpose,
    has_target: bool,
    return_focus: Option<FocusHandle>,
    state: Entity<CommandState>,
    /// The row Enter acts on: the keyboard's, or the last clicked.
    selected: Option<usize>,
    /// Why nothing happened, when the command can't act.
    notice: Option<SharedString>,
    /// The set a delete is waiting on confirmation for.
    confirming: Option<String>,
    on_edit: OnEdit,
    focus_handle: FocusHandle,
}

impl SetPicker {
    fn digits_shown(&self) -> bool {
        self.purpose.switches() && self.has_target
    }

    /// Acts on the set at `index` for this picker's purpose.
    fn choose(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = NamespaceSets::get(cx)
            .sets
            .get(index)
            .map(|set| set.name.clone())
        else {
            return;
        };
        self.selected = Some(index);
        match self.purpose {
            Purpose::Switch | Purpose::SwitchContext if !self.has_target => {
                self.notice = Some(NEEDS_NAMESPACED_PANEL.into());
                cx.notify();
            }
            Purpose::Switch | Purpose::SwitchContext => {
                let action = ApplyNamespaceSet {
                    name,
                    context_wide: self.purpose == Purpose::SwitchContext,
                };
                window.close_dialog(cx);
                if let Some(focus) = &self.return_focus {
                    focus.focus(window, cx);
                }
                window.defer(cx, move |window, cx| {
                    window.dispatch_action(Box::new(action), cx);
                });
            }
            Purpose::Edit => {
                window.close_dialog(cx);
                let on_edit = self.on_edit.clone();
                window.defer(cx, move |window, cx| on_edit(name, window, cx));
            }
            Purpose::Remove => {
                self.confirming = Some(name);
                self.focus_handle.focus(window, cx);
                cx.notify();
            }
        }
    }

    fn confirm_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(name) = self.confirming.take() {
            let _ = NamespaceSets::update(cx, |sets| Ok(sets.remove_set(&name)));
        }
        window.close_dialog(cx);
    }

    /// A digit key applies its set, in the switch modes.
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if !self.purpose.switches() || keystroke.modifiers.modified() {
            return;
        }
        let Some(digit) = keystroke
            .key
            .parse::<usize>()
            .ok()
            .filter(|d| (1..=DIGITS).contains(d))
        else {
            return;
        };
        if NamespaceSets::get(cx).sets.len() < digit {
            return;
        }
        cx.stop_propagation();
        self.choose(digit - 1, window, cx);
    }

    fn render_confirm(&self, name: &str, cx: &mut Context<Self>) -> AnyElement {
        div()
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .gap_3()
            .child(format!("Delete the namespace set “{name}”?"))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new(CANCEL_DELETE_ID)
                            .label("Cancel")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new(CONFIRM_DELETE_ID)
                            .label("Delete")
                            .danger()
                            .on_click(
                                cx.listener(|this, _, window, cx| this.confirm_delete(window, cx)),
                            ),
                    ),
            )
            .into_any_element()
    }
}

impl Render for SetPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(name) = self.confirming.clone() {
            return self.render_confirm(&name, cx);
        }
        let this = cx.weak_entity();
        let muted = cx.theme().muted_foreground;
        let digits = self.digits_shown();
        let sets = NamespaceSets::get(cx).sets.clone();
        let empty = sets.is_empty();
        let items: Vec<CommandItem> = sets
            .iter()
            .enumerate()
            .map(|(index, set)| {
                let name = set.name.clone();
                let summary = set.namespaces.join(", ");
                let digit = (digits && index < DIGITS).then(|| (index + 1).to_string());
                CommandItem::new().label(name.clone()).child(move |_, _| {
                    let selector = row_selector(&name);
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .gap_2()
                        .debug_selector(move || selector)
                        .child(div().w_4().text_color(muted).when_some(
                            digit.clone(),
                            |this, digit| {
                                this.debug_selector(move || digit_selector(index))
                                    .child(digit)
                            },
                        ))
                        .child(name.clone())
                        .child(div().text_sm().text_color(muted).child(summary.clone()))
                })
            })
            .collect();
        let list = CommandList::new(&self.state)
            .items(items)
            .placeholder("Filter sets…")
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
            });
        let notice = self.notice.clone().or_else(|| {
            if empty {
                Some("No namespace sets yet - Create Namespace Set… makes one.".into())
            } else if self.purpose.switches() && !self.has_target {
                Some(NEEDS_NAMESPACED_PANEL.into())
            } else {
                None
            }
        });
        div()
            // Capture, so a digit reaches here before the filter field types it.
            .capture_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_col()
            .gap_2()
            .child(list)
            .children(notice.map(|notice| {
                div()
                    .debug_selector(|| NOTICE_SELECTOR.into())
                    .text_sm()
                    .text_color(muted)
                    .child(notice)
            }))
            .into_any_element()
    }
}
