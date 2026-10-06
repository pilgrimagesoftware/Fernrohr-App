//! The app's one confirmation dialog: a question whose names are set apart
//! ([`ConfirmText`]), Cancel, and a destructive confirm button - each button
//! showing its key from the live keymap, `⏎` and `esc`, in the hint rows'
//! `Kbd` style (`keyboard-first.md`). Delete, Close Window and Close Group all
//! ask through it, so every confirmation reads, looks and keys alike.
//!
//! Two tiers, by [`Severity`] (Fernrohr#168, `keyboard-first.md`):
//! - *Recoverable*: Enter confirms wherever focus is in the dialog - the
//!   dialog's own `Confirm` runs the confirm button's handler, so the `⏎` it
//!   shows is true.
//! - *Irreversible*: the dialog opens with focus on Cancel and Enter cancels.
//!   The danger-styled confirm button runs only when clicked, reached with Tab
//!   and pressed, or on its deliberate shortcut - [`ConfirmIrreversible`],
//!   `cmd-backspace` (`ctrl-backspace` off macOS) by default - which it shows.
//!
//! Escape cancels in both; Tab reaches both buttons, and Enter or Space presses
//! the focused one. Owns the dialog; what confirming does is the caller's.

use crate::ui::confirm_text::ConfirmText;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::dialog::{Cancel, Confirm, DialogFooter};
use gpui_kit::component::h_flex;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::rc::Rc;

actions!(confirm_dialog, [ConfirmIrreversible]);

/// The registered command for [`ConfirmIrreversible`], and its default key.
const CONFIRM_IRREVERSIBLE_ID: &str = "dialog.confirm_irreversible";
const CONFIRM_IRREVERSIBLE_KEY: &str = "secondary-backspace";

/// The key context gpui-component binds the dialog's Enter and Escape in.
const DIALOG_KEY_CONTEXT: &str = "Dialog";
/// Their default keys, the hint's fallback when the keymap has none.
const CONFIRM_KEY: &str = "enter";
const CANCEL_KEY: &str = "escape";

/// How hard what a confirmation confirms is to undo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Severity {
    /// Gone for good - a deleted cluster object, a discarded unsaved edit:
    /// Enter cancels, and confirming takes a deliberate press.
    Irreversible,
    /// Can be redone - a stopped forward, a disconnect, an ended shell: Enter
    /// confirms.
    Recoverable,
}

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
    pub(crate) severity: Severity,
}

/// Registers [`ConfirmIrreversible`], so its key is bound like every other
/// command's and `keymap.toml` can rebind it.
pub fn register_commands(registry: &mut crate::command::CommandRegistry) {
    registry.register(crate::command::Command {
        id: CONFIRM_IRREVERSIBLE_ID,
        title: "Confirm Irreversible Action",
        default_binding: CONFIRM_IRREVERSIBLE_KEY,
        context: Some(DIALOG_KEY_CONTEXT),
        action: Box::new(ConfirmIrreversible),
        menu: None,
    });
}

/// The ids [`open`] gives `id_prefix`'s buttons.
pub(crate) fn cancel_id(id_prefix: &str) -> SharedString {
    format!("{id_prefix}-cancel").into()
}
pub(crate) fn confirm_id(id_prefix: &str) -> SharedString {
    format!("{id_prefix}-confirm").into()
}

/// Asks `confirmation`, running `on_confirm` only if the user confirms - the
/// confirm button, or the tier's key: Enter when recoverable, the
/// [`ConfirmIrreversible`] shortcut when not. Cancel, or Escape, does nothing.
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
        severity,
    } = confirmation;
    let irreversible = severity == Severity::Irreversible;
    window.open_dialog(cx, move |dialog, window, cx| {
        let (on_ok, on_click) = (on_confirm.clone(), on_confirm.clone());
        let (on_enter, on_shortcut) = (on_confirm.clone(), on_confirm.clone());
        let confirm_key = if irreversible {
            button_key(&ConfirmIrreversible, CONFIRM_IRREVERSIBLE_KEY, window)
        } else {
            button_key(&Confirm { secondary: false }, CONFIRM_KEY, window)
        };
        dialog
            .title(title.clone())
            .child(body.render(cx))
            // Enter: when recoverable, closes, then acts - the click path's
            // order - and returns false, as the dialog is already closed.
            // Closing after acting would close any dialog the action opened
            // instead of this one. When irreversible, Enter only closes.
            .on_ok(move |_event, window, cx| {
                window.close_dialog(cx);
                if !irreversible {
                    on_ok(window, cx);
                }
                false
            })
            .footer(
                DialogFooter::new().child(
                    // The buttons' parent, so the shortcut is heard from either.
                    h_flex()
                        .gap_2()
                        .when(irreversible, |buttons| {
                            buttons.on_action(move |_: &ConfirmIrreversible, window, cx| {
                                window.close_dialog(cx);
                                on_shortcut(window, cx);
                            })
                        })
                        // Each button's parent hears the dialog's Enter
                        // (`Confirm`) first while that button has focus, so
                        // Enter presses the focused button - Cancel cancels,
                        // even when recoverable.
                        .child(
                            div()
                                .on_action(|_: &Confirm, window, cx| window.close_dialog(cx))
                                .child(
                                    Button::new(cancel_id(id_prefix))
                                        .label("Cancel")
                                        .child(button_key(&Cancel, CANCEL_KEY, window))
                                        .on_click(|_event, window, cx| window.close_dialog(cx)),
                                ),
                        )
                        .child(
                            div()
                                .on_action(move |_: &Confirm, window, cx| {
                                    window.close_dialog(cx);
                                    on_enter(window, cx);
                                })
                                .child(
                                    Button::new(confirm_id(id_prefix))
                                        .label(confirm.clone())
                                        .child(confirm_key)
                                        .with_variant(ButtonVariant::Danger)
                                        .on_click(move |_event, window, cx| {
                                            window.close_dialog(cx);
                                            on_click(window, cx);
                                        }),
                                ),
                        ),
                ),
            )
    });
    if irreversible {
        // Cancel is the dialog's first tab stop; once it is drawn, start there,
        // so the button Enter presses is Cancel. Only while this dialog still
        // holds the focus it opened with: closed before its first frame, or
        // already tabbed through, it moves nothing.
        let opened_with = window.focused(cx);
        window.on_next_frame(move |window, cx| {
            if opened_with.is_some_and(|focus| focus.is_focused(window)) {
                window.focus_next(cx);
            }
        });
    }
}

/// Enter's and Escape's keys in a dialog, for another dialog's own confirm and
/// cancel buttons to show as these do.
pub(crate) fn confirm_key(window: &Window) -> Kbd {
    button_key(&Confirm { secondary: false }, CONFIRM_KEY, window)
}
pub(crate) fn cancel_key(window: &Window) -> Kbd {
    button_key(&Cancel, CANCEL_KEY, window)
}

/// Draws a frame and delivers the next one, as the platform would after a
/// dialog opens - tests have no frame loop - so an irreversible dialog's focus
/// moves to Cancel. Test-only.
#[cfg(test)]
pub(crate) fn deliver_first_frame(vcx: &mut VisualTestContext) {
    use gpui_kit::test::TestWindowExt as _;
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        window.render_frame(cx);
        window.simulate_next_frame(cx);
        window.render_frame(cx);
    });
    vcx.run_until_parked();
}

/// The key that triggers `action` in a dialog, as the keymap binds it -
/// `fallback` if it binds none - drawn as the hint rows draw keys.
fn button_key(action: &dyn Action, fallback: &str, window: &Window) -> Kbd {
    Kbd::binding_for_action(action, Some(DIALOG_KEY_CONTEXT), window)
        .unwrap_or_else(|| Kbd::new(Keystroke::parse(fallback).expect("valid keybinding")))
}

#[cfg(test)]
mod tests;
