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

use crate::command::CommandRegistry;
use gpui_kit::component::ActiveTheme as _;
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
    let contexts: Vec<SharedString> = window
        .context_stack()
        .iter()
        .filter_map(|context| context.primary().map(|entry| entry.key.clone()))
        .collect();
    let context_names: Vec<&str> = contexts.iter().map(|name| name.as_ref()).collect();

    let registry = cx.global::<CommandRegistry>();
    let commands = registry.available(&context_names);
    // Items carry no `action`: `Command` would dispatch it from inside the dialog,
    // missing panel-scoped handlers. So each row draws its own key - looked up
    // against the element that had focus, where panel-scoped keys resolve.
    let selected = Rc::new(Cell::new((!commands.is_empty()).then_some(0usize)));
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
            CommandItem::new().label(title).child(move |_window, cx| {
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
                    .child(title)
                    .children(key.clone())
                    .into_any_element()
            })
        })
        .collect();
    let actions: Vec<Box<dyn Action>> = commands
        .iter()
        .map(|command| command.action.boxed_clone())
        .collect();

    let state = cx.new(|cx| CommandState::new(window, cx));
    let search_focus = state.read(cx).focus_handle(cx);
    window.open_dialog(cx, move |dialog, _window, _cx| {
        let state = state.clone();
        let items = items.clone();
        let actions: Vec<Box<dyn Action>> =
            actions.iter().map(|action| action.boxed_clone()).collect();
        let previous_focus = previous_focus.clone();
        let selected = selected.clone();
        dialog.content(move |content, _window, _cx| {
            let actions: Vec<Box<dyn Action>> =
                actions.iter().map(|action| action.boxed_clone()).collect();
            let previous_focus = previous_focus.clone();
            let on_select_selected = selected.clone();
            let on_select_state = state.clone();
            let confirm_selected = selected.clone();
            content.child(
                Command::new(&state)
                    .items(items.clone())
                    .placeholder("Type a command...")
                    // Only keyboard moves (arrows, typing) move the selection;
                    // `Command` reports hover the same way, and hover must not.
                    .on_select(move |index_path, window, cx| {
                        if window.last_input_was_keyboard() {
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
mod tests {
    use super::open;
    use crate::command::{Command, CommandRegistry};
    use gpui_kit::component::Root;
    use gpui_kit::{
        AppContext as _, Context, FocusHandle, IntoElement, ParentElement as _, Render,
        TestAppContext, VisualTestContext, Window, actions, div,
    };
    use std::cell::Cell;
    use std::rc::Rc;

    actions!(palette_test, [First, Second]);

    struct Host {
        focus: FocusHandle,
    }

    impl Render for Host {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            use gpui_kit::InteractiveElement as _;
            div()
                .track_focus(&self.focus)
                .children(Root::render_dialog_layer(window, cx))
        }
    }

    /// Down moves through the filtered list and Enter runs the selected command.
    #[gpui_kit::test]
    async fn arrows_select_and_enter_runs(cx: &mut TestAppContext) {
        let ran_second = Rc::new(Cell::new(false));
        cx.update(|cx| {
            gpui_kit::init(cx);
            let mut registry = CommandRegistry::new();
            for (id, title, action) in [
                (
                    "test.first",
                    "First Test Command",
                    Box::new(First) as Box<dyn gpui_kit::Action>,
                ),
                ("test.second", "Second Test Command", Box::new(Second)),
            ] {
                registry.register(Command {
                    id,
                    title,
                    default_binding: "",
                    context: None,
                    action,
                    menu: None,
                });
            }
            cx.set_global(registry);
            let flag = ran_second.clone();
            cx.on_action(move |_: &Second, _cx| flag.set(true));
        });
        let window = cx.add_window(|window, cx| {
            let host = cx.new(|cx| Host {
                focus: cx.focus_handle(),
            });
            let focus = host.read(cx).focus.clone();
            window.focus(&focus, cx);
            Root::new(host, window, cx)
        });
        let mut vcx = VisualTestContext::from_window(window.into(), cx);
        vcx.update(open);
        vcx.run_until_parked();

        vcx.simulate_keystrokes("down enter");
        vcx.run_until_parked();
        assert!(
            ran_second.get(),
            "Down selects the second command and Enter runs it"
        );
    }

    /// Typing filters and moves the selection to the first match; Enter runs it.
    #[gpui_kit::test]
    async fn typing_selects_the_first_match_and_enter_runs_it(cx: &mut TestAppContext) {
        let ran_second = Rc::new(Cell::new(false));
        cx.update(|cx| {
            gpui_kit::init(cx);
            let mut registry = CommandRegistry::new();
            for (id, title, action) in [
                (
                    "test.first",
                    "First Test Command",
                    Box::new(First) as Box<dyn gpui_kit::Action>,
                ),
                ("test.second", "Second Test Command", Box::new(Second)),
            ] {
                registry.register(Command {
                    id,
                    title,
                    default_binding: "",
                    context: None,
                    action,
                    menu: None,
                });
            }
            cx.set_global(registry);
            let flag = ran_second.clone();
            cx.on_action(move |_: &Second, _cx| flag.set(true));
        });
        let window = cx.add_window(|window, cx| {
            let host = cx.new(|cx| Host {
                focus: cx.focus_handle(),
            });
            let focus = host.read(cx).focus.clone();
            window.focus(&focus, cx);
            Root::new(host, window, cx)
        });
        let mut vcx = VisualTestContext::from_window(window.into(), cx);
        vcx.update(open);
        vcx.run_until_parked();

        vcx.simulate_input("second");
        vcx.run_until_parked();
        vcx.simulate_keystrokes("enter");
        vcx.run_until_parked();
        assert!(ran_second.get(), "the filter's first match runs on Enter");
    }
}
