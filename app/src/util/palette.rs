//! The command palette: every registered command available where focus is, in one
//! searchable dialog - the keyboard's catch-all (`keyboard-first.md`).
//!
//! Three things make it work from the keyboard, and each was once missing:
//! - It lists the commands for the *focused* element's key contexts, read from GPUI's
//!   context stack before the dialog takes focus, so panel-scoped commands appear.
//! - Its search box takes focus as the dialog opens, so typing filters at once.
//! - A chosen command runs where focus was: the palette closes, focus returns to the
//!   element that had it, and the action is dispatched there. Dispatching from inside
//!   the dialog would miss every panel-scoped action.
//! - It keeps its own selection, moved by the keyboard (arrows, or typing to filter)
//!   and drawn in the theme's selection color. `Command`'s built-in highlight is
//!   nearly invisible in a dialog and follows the mouse, so Enter could run whatever
//!   was under the pointer; here Enter runs the keyboard selection and a click runs
//!   the row clicked.
//! - A command the focused panel says can't run now (`command::unavailable`, #406)
//!   is listed disabled - dimmed, with the reason beside its title - and never runs:
//!   neither the arrows nor a click select it, and Enter skips it.

use crate::command::CommandRegistry;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Disableable as _;
use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::command::{Command, CommandItem, CommandState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::cell::Cell;
use std::rc::Rc;

/// Opens the palette over `window`'s current focus.
pub fn open(window: &mut Window, cx: &mut App) {
    // `WindowExt`'s dialog calls expect a `Root`; a window without one has
    // nowhere to show the dialog.
    if !matches!(window.root::<Root>(), Some(Some(_))) {
        return;
    }
    let previous_focus = window.focused(cx);
    // Every identifier on the path, not only each element's primary: a panel
    // adds conditional contexts beside its own (`PodDetailShellable`,
    // `DeletableObject`), and their commands belong in the palette too.
    let contexts: Vec<SharedString> = window
        .context_stack()
        .iter()
        .flat_map(|context| context.primary().into_iter().chain(context.secondary()))
        .filter(|entry| entry.value.is_none())
        .map(|entry| entry.key.clone())
        .collect();
    let context_names: Vec<&str> = contexts.iter().map(|name| name.as_ref()).collect();

    let registry = cx.global::<CommandRegistry>();
    let commands = registry.available(&context_names);
    // Asked now, while focus is still where the palette was summoned from.
    let reasons: Vec<Option<SharedString>> = commands
        .iter()
        .map(|command| crate::command::unavailable::reason(command.action.as_ref(), window, cx))
        .collect();
    // Items carry no `action`: `Command` would dispatch it from inside the dialog,
    // missing panel-scoped handlers. So each row draws its own key - looked up
    // against the element that had focus, where panel-scoped keys resolve.
    let selected = Rc::new(Cell::new(reasons.iter().position(Option::is_none)));
    let items: Vec<CommandItem> = commands
        .iter()
        .enumerate()
        .map(|(row, command)| {
            let key = previous_focus
                .as_ref()
                .and_then(|focus| {
                    Kbd::binding_for_action_in(command.action.as_ref(), focus, window)
                })
                .or_else(|| Kbd::binding_for_action(command.action.as_ref(), None, window));
            let title = command.title;
            let selected = selected.clone();
            let reason = reasons[row].clone();
            let disabled = reason.is_some();
            CommandItem::new()
                .label(title)
                .disabled(disabled)
                .child(move |_window, cx| {
                    div()
                        .flex()
                        .flex_1()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .px_1()
                        .rounded(cx.theme().radius)
                        .when(selected.get() == Some(row), |el| {
                            el.bg(crate::ui::style::accent_subtle(cx))
                        })
                        .when(disabled, |el| el.text_color(cx.theme().muted_foreground))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .min_w_0()
                                .child(title)
                                .children(reason.clone().map(|reason| {
                                    div()
                                        .debug_selector(move || format!("palette-reason-{row}"))
                                        .text_xs()
                                        .truncate()
                                        .child(reason)
                                })),
                        )
                        .children(key.clone())
                        .into_any_element()
                })
        })
        .collect();
    let actions: Vec<Box<dyn Action>> = commands
        .iter()
        .map(|command| command.action.boxed_clone())
        .collect();
    let runnable: Rc<[bool]> = reasons.iter().map(Option::is_none).collect();

    let state = cx.new(|cx| CommandState::new(window, cx));
    let search_focus = state.read(cx).focus_handle(cx);
    window.open_dialog(cx, move |dialog, _window, _cx| {
        let state = state.clone();
        let items = items.clone();
        let actions: Vec<Box<dyn Action>> =
            actions.iter().map(|action| action.boxed_clone()).collect();
        let previous_focus = previous_focus.clone();
        let selected = selected.clone();
        let runnable = runnable.clone();
        dialog.content(move |content, _window, _cx| {
            let actions: Vec<Box<dyn Action>> =
                actions.iter().map(|action| action.boxed_clone()).collect();
            let previous_focus = previous_focus.clone();
            let on_select_selected = selected.clone();
            let on_select_state = state.clone();
            let select_runnable = runnable.clone();
            let on_select_runnable =
                move |row: usize| select_runnable.get(row).copied().unwrap_or(false);
            let confirm_selected = selected.clone();
            let runnable = runnable.clone();
            content.child(
                Command::new(&state)
                    .items(items.clone())
                    .placeholder("Type a command...")
                    // Only keyboard moves (arrows, typing) move the selection;
                    // `Command` reports hover the same way, and hover must not.
                    .on_select(move |index_path, window, cx| {
                        if window.last_input_was_keyboard() && on_select_runnable(index_path.row) {
                            on_select_selected.set(Some(index_path.row));
                            on_select_state.update(cx, |_, cx| cx.notify());
                        }
                    })
                    .on_confirm(move |index_path, window, cx| {
                        // Enter runs the keyboard selection; a click, the row clicked.
                        let row = if window.last_input_was_keyboard() {
                            confirm_selected.get().unwrap_or(index_path.row)
                        } else {
                            index_path.row
                        };
                        // Belt and braces: `Command` doesn't confirm a disabled row,
                        // but the kept selection must not run one either.
                        if !runnable.get(row).copied().unwrap_or(false) {
                            return;
                        }
                        run(&actions, row, previous_focus.as_ref(), window, cx)
                    }),
            )
        })
    });
    // After the dialog has taken focus for itself, hand it to the search box.
    window.defer(cx, move |window, cx| window.focus(&search_focus, cx));
}

/// Closes the palette, returns focus to where it was, and runs the chosen command
/// there, so a panel-scoped action reaches its panel.
fn run(
    actions: &[Box<dyn Action>],
    row: usize,
    previous_focus: Option<&FocusHandle>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(action) = actions.get(row).map(|action| action.boxed_clone()) else {
        return;
    };
    window.close_dialog(cx);
    if let Some(focus) = previous_focus {
        window.focus(focus, cx);
    }
    window.dispatch_action(action, cx);
}

#[cfg(test)]
mod tests;
