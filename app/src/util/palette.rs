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

use crate::command::CommandRegistry;
use gpui_kit::component::Root;
use gpui_kit::component::command::{Command, CommandItem, CommandState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;

/// Opens the palette over `window`'s current focus.
pub fn open(window: &mut Window, cx: &mut App) {
    let Some(Some(root)) = window.root::<Root>() else {
        return;
    };
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
    let items: Vec<CommandItem> = commands
        .iter()
        .map(|command| {
            let key = previous_focus
                .as_ref()
                .and_then(|focus| {
                    Kbd::binding_for_action_in(command.action.as_ref(), focus, window)
                })
                .or_else(|| Kbd::binding_for_action(command.action.as_ref(), None, window));
            let title = command.title;
            CommandItem::new().label(title).child(move |_window, _cx| {
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_between()
                    .gap_2()
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
    root.update(cx, |root, cx| {
        root.open_dialog(
            move |dialog, _window, _cx| {
                let state = state.clone();
                let items = items.clone();
                let actions: Vec<Box<dyn Action>> =
                    actions.iter().map(|action| action.boxed_clone()).collect();
                let previous_focus = previous_focus.clone();
                dialog.content(move |content, _window, _cx| {
                    let actions: Vec<Box<dyn Action>> =
                        actions.iter().map(|action| action.boxed_clone()).collect();
                    let previous_focus = previous_focus.clone();
                    content.child(
                        Command::new(&state)
                            .items(items.clone())
                            .placeholder("Type a command...")
                            .on_confirm(move |index_path, window, cx| {
                                run(
                                    &actions,
                                    index_path.row,
                                    previous_focus.as_ref(),
                                    window,
                                    cx,
                                )
                            }),
                    )
                })
            },
            window,
            cx,
        );
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
    Root::update(window, cx, |root, window, cx| root.close_dialog(window, cx));
    if let Some(focus) = previous_focus {
        window.focus(focus, cx);
    }
    window.dispatch_action(action, cx);
}
