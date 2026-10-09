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
//!
//! [`open_with`] adds what a caller asking on someone else's behalf needs
//! (`agent-mcp`'s action approval): labelled detail rows under the question,
//! and a callback for every way the dialog is turned down.

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

/// One labelled row under a confirmation's question: "Replicas", "2 → 3".
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Detail {
    pub(crate) label: SharedString,
    pub(crate) value: SharedString,
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
    open_with(
        confirmation,
        Vec::new(),
        on_confirm,
        |_window, _cx| {},
        window,
        cx,
    );
}

/// [`open`], with `details` listed under the question, and `on_cancel` run
/// when the user turns it down - Cancel, Escape, or Enter on Cancel.
pub(crate) fn open_with(
    confirmation: Confirmation,
    details: Vec<Detail>,
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let on_confirm = Rc::new(on_confirm);
    let on_cancel = Rc::new(on_cancel);
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
        let (cancel_on_ok, cancel_on_escape) = (on_cancel.clone(), on_cancel.clone());
        let (cancel_on_enter, cancel_on_click) = (on_cancel.clone(), on_cancel.clone());
        let confirm_key = if irreversible {
            button_key(&ConfirmIrreversible, CONFIRM_IRREVERSIBLE_KEY, window)
        } else {
            button_key(&Confirm { secondary: false }, CONFIRM_KEY, window)
        };
        dialog
            .title(title.clone())
            .child(body.render(cx))
            .when(!details.is_empty(), |dialog| {
                dialog.child(render_details(&details, cx))
            })
            // Escape, and the dialog's own close control.
            .on_cancel(move |_event, window, cx| {
                cancel_on_escape(window, cx);
                true
            })
            // Enter: when recoverable, closes, then acts - the click path's
            // order - and returns false, as the dialog is already closed.
            // Closing after acting would close any dialog the action opened
            // instead of this one. When irreversible, Enter only closes.
            .on_ok(move |_event, window, cx| {
                window.close_dialog(cx);
                if irreversible {
                    cancel_on_ok(window, cx);
                } else {
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
                                .on_action(move |_: &Confirm, window, cx| {
                                    window.close_dialog(cx);
                                    cancel_on_enter(window, cx);
                                })
                                .child(
                                    Button::new(cancel_id(id_prefix))
                                        .label("Cancel")
                                        .child(button_key(&Cancel, CANCEL_KEY, window))
                                        .on_click(move |_event, window, cx| {
                                            window.close_dialog(cx);
                                            cancel_on_click(window, cx);
                                        }),
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
        if let Some(dialog) = window.focused(cx) {
            window.on_next_frame(move |window, cx| {
                if dialog.is_focused(window) {
                    focus_first_stop_in(&dialog, window, cx);
                }
            });
        }
    }
}

/// `details` as label/value rows: labels muted, values as they are, long
/// ones wrapping.
fn render_details(details: &[Detail], cx: &App) -> impl IntoElement {
    use gpui_kit::component::{ActiveTheme as _, v_flex};
    let muted = cx.theme().muted_foreground;
    v_flex()
        .id("confirm-details")
        .mt_2()
        .gap_1()
        .children(details.iter().map(|detail| {
            h_flex()
                .gap_3()
                .items_start()
                .child(
                    div()
                        .w(px(96.))
                        .flex_none()
                        .text_color(muted)
                        .child(detail.label.clone()),
                )
                .child(div().flex_1().min_w_0().child(detail.value.clone()))
        }))
}

/// Moves focus to the first tab stop inside `dialog`. Tab order runs through
/// the whole window, and the dialog draws last, so the stop after it may be
/// one behind it: step on until focus is back inside - as gpui-base's own Tab
/// does in a focus trap - and leave it on `dialog` if no stop inside is found.
fn focus_first_stop_in(dialog: &FocusHandle, window: &mut Window, cx: &mut App) {
    /// More stops than any window here has, so a lap always comes back round.
    const MAX_STEPS: usize = 200;
    for _ in 0..MAX_STEPS {
        window.focus_next(cx);
        if dialog.contains_focused(window, cx) && !dialog.is_focused(window) {
            return;
        }
    }
    window.focus(dialog, cx);
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
