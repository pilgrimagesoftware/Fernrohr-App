//! The app's one confirmation dialog: a question whose names are set apart
//! ([`ConfirmText`]), Cancel, and a destructive confirm button - each button
//! showing its key from the live keymap, `⏎` and `esc`, in the hint rows'
//! `Kbd` style (`keyboard-first.md`). Delete, Close Window and Close Group all
//! ask through it, so every confirmation reads, looks and keys alike.
//!
//! Enter confirms and Escape cancels wherever focus is in the dialog: the
//! dialog's own `Confirm` runs the same handler the confirm button's click
//! does, so the `⏎` it shows is true. Tab still reaches both buttons, and
//! Space presses the focused one. Owns the dialog; what confirming does is the
//! caller's.

use crate::ui::confirm_text::ConfirmText;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::dialog::{Cancel, Confirm, DialogFooter};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;
use std::rc::Rc;

/// The key context gpui-component binds the dialog's Enter and Escape in.
const DIALOG_KEY_CONTEXT: &str = "Dialog";
/// Their default keys, the hint's fallback when the keymap has none.
const CONFIRM_KEY: &str = "enter";
const CANCEL_KEY: &str = "escape";

/// What a confirmation asks.
pub(crate) struct Confirmation {
    /// "Delete Secret?"
    pub(crate) title: SharedString,
    /// What confirming does or loses, naming the objects it touches.
    pub(crate) body: ConfirmText,
    /// The confirm button's label: "Delete".
    pub(crate) confirm: SharedString,
    /// The buttons' ids are `<id_prefix>-cancel` and `<id_prefix>-confirm`.
    pub(crate) id_prefix: &'static str,
}

/// The ids [`open`] gives `id_prefix`'s buttons.
pub(crate) fn cancel_id(id_prefix: &str) -> SharedString {
    format!("{id_prefix}-cancel").into()
}
pub(crate) fn confirm_id(id_prefix: &str) -> SharedString {
    format!("{id_prefix}-confirm").into()
}

/// Asks `confirmation`, running `on_confirm` only if the user confirms - the
/// confirm button, or Enter. Cancel, or Escape, does nothing.
pub(crate) fn open(
    confirmation: Confirmation,
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let on_confirm = Rc::new(on_confirm);
    let Confirmation {
        title,
        body,
        confirm,
        id_prefix,
    } = confirmation;
    window.open_dialog(cx, move |dialog, window, cx| {
        let (on_ok, on_click) = (on_confirm.clone(), on_confirm.clone());
        dialog
            .title(title.clone())
            .child(body.render(cx))
            // Enter: closes, then acts - the click path's order - and returns
            // false, as the dialog is already closed. Closing after acting would
            // close any dialog the action opened instead of this one.
            .on_ok(move |_event, window, cx| {
                window.close_dialog(cx);
                on_ok(window, cx);
                false
            })
            .footer(
                DialogFooter::new()
                    .child(
                        Button::new(cancel_id(id_prefix))
                            .label("Cancel")
                            .child(button_key(&Cancel, CANCEL_KEY, window))
                            .on_click(|_event, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new(confirm_id(id_prefix))
                            .label(confirm.clone())
                            .child(button_key(
                                &Confirm { secondary: false },
                                CONFIRM_KEY,
                                window,
                            ))
                            .with_variant(ButtonVariant::Danger)
                            .on_click(move |_event, window, cx| {
                                window.close_dialog(cx);
                                on_click(window, cx);
                            }),
                    ),
            )
    });
}

/// Enter's and Escape's keys in a dialog, for another dialog's own confirm and
/// cancel buttons to show as these do.
pub(crate) fn confirm_key(window: &Window) -> Kbd {
    button_key(&Confirm { secondary: false }, CONFIRM_KEY, window)
}
pub(crate) fn cancel_key(window: &Window) -> Kbd {
    button_key(&Cancel, CANCEL_KEY, window)
}

/// The key that triggers `action` in a dialog, as the keymap binds it -
/// `fallback` if it binds none - drawn as the hint rows draw keys.
fn button_key(action: &dyn Action, fallback: &str, window: &Window) -> Kbd {
    Kbd::binding_for_action(action, Some(DIALOG_KEY_CONTEXT), window)
        .unwrap_or_else(|| Kbd::new(Keystroke::parse(fallback).expect("valid keybinding")))
}

#[cfg(test)]
mod tests;
