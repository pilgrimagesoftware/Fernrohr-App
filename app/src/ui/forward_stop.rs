//! The one confirmation every way of stopping a port-forward goes through
//! (`port-forward-indicators` 5.2): it names the pod or Service, where the forward
//! listens and the port it reaches - the names set apart from the sentence - and
//! stops it only once confirmed.
//!
//! Keyboard-first: Enter confirms and Escape cancels (the dialog's own keys),
//! Tab moves between the buttons, and each button shows its key as a `Kbd`, read
//! from the live keymap like a hint row's, as Knot's permission prompt does.

use gpui_kit::base::StyledExt as _;
use gpui_kit::base::actions::{Cancel, Confirm};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;
use std::net::SocketAddr;
use std::rc::Rc;

/// The dialog's key context, where Enter and Escape are bound.
const DIALOG_CONTEXT: &str = "Dialog";
/// The confirmation's buttons, for tests.
pub const CANCEL_ID: &str = "forward-stop-cancel";
pub const CONFIRM_ID: &str = "forward-stop-confirm";

/// What is being stopped: `kind` and `name` of the object it forwards to, where
/// it listens, and the port it reaches.
pub struct StopTarget {
    pub kind: &'static str,
    pub name: String,
    pub local_addr: SocketAddr,
    pub target_port: u16,
}

/// Asks before stopping `target`'s forward; `stop` runs only on confirmation.
pub fn confirm_stop(
    target: StopTarget,
    stop: impl Fn(&mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let stop = Rc::new(stop);
    window.open_dialog(cx, move |dialog, window, _cx| {
        let (on_ok, on_click) = (stop.clone(), stop.clone());
        let name = |text: String| div().font_semibold().child(text);
        let body = div()
            .flex()
            .flex_wrap()
            .gap_x_1()
            .child("Stop forwarding")
            .child(name(target.local_addr.to_string()))
            .child(format!("to {}", target.kind))
            .child(name(format!("{}:{}", target.name, target.target_port)))
            .child("? Anything connected through it is disconnected.");
        let cancel_key = Kbd::binding_for_action(&Cancel, Some(DIALOG_CONTEXT), window);
        let confirm_key =
            Kbd::binding_for_action(&Confirm { secondary: false }, Some(DIALOG_CONTEXT), window);
        dialog
            .title("Stop Port Forward?")
            .child(body)
            .on_ok(move |_, window, cx| {
                on_ok(window, cx);
                true
            })
            .footer(
                DialogFooter::new()
                    .child(
                        Button::new(CANCEL_ID)
                            .label("Cancel")
                            .children(cancel_key)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new(CONFIRM_ID)
                            .label("Stop Forward")
                            .with_variant(ButtonVariant::Danger)
                            .children(confirm_key)
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                on_click(window, cx);
                            }),
                    ),
            )
    });
}
