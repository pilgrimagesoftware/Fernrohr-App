//! The key hints overlay (`k9s-remaining-keybindings` 7): `?` lists every
//! command available where focus is, each with its current key - the palette's
//! set, read the same way from the focused element's context stack, so a new
//! command is listed without registering it twice. A command with no key is
//! still listed, as unbound.
//!
//! Read-only: nothing runs from it. Escape or a click outside dismisses it, and
//! the window's `Root` hands focus back to the element that had it.

use crate::command::{Command, CommandRegistry};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;

actions!(key_hints, [ShowKeyHints]);

pub const SHOW_KEY_HINTS_COMMAND_ID: &str = "global.show_key_hints";
/// k9s's help key. Bound outside text fields, where `?` is just a character.
const SHOW_KEY_HINTS_KEY: &str = "?";
const SHOW_KEY_HINTS_CONTEXT: &str = "!Input";

/// What a row of the overlay shows for a command.
#[derive(Clone, Debug, PartialEq)]
pub struct HintRow {
    pub id: &'static str,
    pub title: &'static str,
    /// The key as the keymap binds it here, or `None` when unbound.
    pub key: Option<String>,
}

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: SHOW_KEY_HINTS_COMMAND_ID,
        title: "Show Key Hints",
        default_binding: SHOW_KEY_HINTS_KEY,
        context: Some(SHOW_KEY_HINTS_CONTEXT),
        action: Box::new(ShowKeyHints),
        // Not in the menu bar: it's bound outside text fields (`!Input`), and
        // menu items are global (`menu-organization`).
        menu: None,
    });
}

/// The rows for where focus is now: every command the palette would offer
/// there, with its key resolved against the focused element.
pub fn rows(window: &Window, cx: &App) -> Vec<HintRow> {
    let focus = window.focused(cx);
    let contexts: Vec<SharedString> = window
        .context_stack()
        .iter()
        .filter_map(|context| context.primary().map(|entry| entry.key.clone()))
        .collect();
    let names: Vec<&str> = contexts.iter().map(|name| name.as_ref()).collect();
    cx.global::<CommandRegistry>()
        .available(&names)
        .into_iter()
        .map(|command| HintRow {
            id: command.id,
            title: command.title,
            key: key_for(command.action.as_ref(), focus.as_ref(), window),
        })
        .collect()
}

/// The first key `action` is bound to where `focus` is - or, with nothing
/// focused, anywhere - as keystroke text (`ctrl-d`), or `None` when unbound.
fn key_for(action: &dyn Action, focus: Option<&FocusHandle>, window: &Window) -> Option<String> {
    let bindings = match focus {
        Some(focus) => window.bindings_for_action_in(action, focus),
        None => window.bindings_for_action(action),
    };
    let binding = bindings.into_iter().next()?;
    let keys: Vec<String> = binding
        .keystrokes()
        .iter()
        .map(|keystroke| keystroke.unparse())
        .collect();
    (!keys.is_empty()).then(|| keys.join(" "))
}

/// The debug selector of a command's row.
pub fn row_selector(id: &str) -> String {
    format!("key-hint {id}")
}

/// Opens the overlay over `window`'s current focus.
pub fn open(window: &mut Window, cx: &mut App) {
    if !matches!(window.root::<Root>(), Some(Some(_))) {
        return;
    }
    let rows = rows(window, cx);
    window.open_dialog(cx, move |dialog, _window, cx| {
        let muted = cx.theme().muted_foreground;
        let list = rows.iter().map(|row| {
            let selector = row_selector(row.id);
            div()
                .debug_selector(move || selector)
                .flex()
                .items_center()
                .justify_between()
                .gap_4()
                .py_0p5()
                .child(row.title)
                .child(
                    match row
                        .key
                        .as_deref()
                        .and_then(|key| key.split(' ').next())
                        .and_then(|key| Keystroke::parse(key).ok())
                    {
                        Some(keystroke) => Kbd::new(keystroke).into_any_element(),
                        None => div()
                            .text_sm()
                            .text_color(muted)
                            .child("Unbound")
                            .into_any_element(),
                    },
                )
        });
        dialog.title("Key Hints").child(
            div()
                .id("key-hints-list")
                .max_h(px(480.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .children(list),
        )
    });
}

#[cfg(test)]
mod tests;
