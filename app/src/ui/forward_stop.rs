//! The one confirmation every way of stopping a port-forward goes through
//! (`port-forward-indicators` 5.2): the app's shared confirmation
//! (`ui::confirm_dialog`), naming the pod or Service, where the forward listens
//! and the port it reaches, and stopping it only once confirmed. Enter or Stop
//! Forward stops it; Escape or Cancel keeps it.
//!
//! [`stop_forward`] is that whole path for one forward; [`stop_one_of`] is the
//! Stop Port Forward command's, asking which first when an object has several.

use crate::k8s::cluster::port_forwards::{ForwardSummary, PortForwards};
use crate::ui::confirm_dialog::{self, Confirmation};
use crate::ui::confirm_text::ConfirmText;
use gpui_kit::component::WindowExt as _;
use gpui_kit::*;
use std::net::SocketAddr;

/// The confirmation's id prefix: its buttons are [`cancel_id`] and [`confirm_id`].
const ID_PREFIX: &str = "forward-stop";

#[cfg(test)]
pub fn cancel_id() -> SharedString {
    confirm_dialog::cancel_id(ID_PREFIX)
}

#[cfg(test)]
pub fn confirm_id() -> SharedString {
    confirm_dialog::confirm_id(ID_PREFIX)
}

/// What is being stopped: the pod it reaches and the Service it was started
/// from, if any, where it listens, and the port it reaches.
pub struct StopTarget {
    pub pod: String,
    pub via_service: Option<String>,
    pub local_addr: SocketAddr,
    pub target_port: u16,
}

/// The question, the names set apart.
fn body(target: &StopTarget) -> ConfirmText {
    let text = ConfirmText::new()
        .text("Stop forwarding ")
        .name(&target.local_addr.to_string())
        .text(" to ");
    let text = match &target.via_service {
        Some(service) => text.text("service ").name(service).text(" \u{2192} pod "),
        None => text.text("pod "),
    };
    text.name(&format!("{}:{}", target.pod, target.target_port))
        .text("? Anything connected through it is disconnected.")
}

/// Asks before stopping `target`'s forward; `stop` runs only on confirmation.
pub fn confirm_stop(
    target: StopTarget,
    stop: impl Fn(&mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let confirmation = Confirmation {
        title: "Stop Port Forward?".into(),
        body: body(&target),
        confirm: "Stop Forward".into(),
        id_prefix: ID_PREFIX,
        severity: crate::ui::confirm_dialog::Severity::Recoverable,
    };
    confirm_dialog::open(confirmation, stop, window, cx);
}

/// Asks, then stops `forward` - naming its pod, and the Service it was started
/// from when it was.
pub fn stop_forward(forward: &ForwardSummary, window: &mut Window, cx: &mut App) {
    let via_service = PortForwards::existing(cx)
        .and_then(|forwards| forwards.read(cx).via_service(&forward.request));
    let target = StopTarget {
        pod: forward.request.pod.clone(),
        via_service,
        local_addr: forward.local_addr,
        target_port: forward.target_port,
    };
    let request = forward.request.clone();
    confirm_stop(
        target,
        move |_window, cx| {
            PortForwards::entity(cx).update(cx, |forwards, cx| {
                forwards.stop(&request, cx);
            });
        },
        window,
        cx,
    );
}

/// The picker's buttons, for tests.
pub fn pick_button_id(index: usize) -> SharedString {
    format!("forward-stop-pick-{index}").into()
}
pub const PICK_CANCEL_ID: &str = "forward-stop-pick-cancel";

/// Stop Port Forward on `kind` `name`, whose forwards are `forwards`: nothing
/// with none, the confirmation with one, and with several a choice of which
/// first - each forward a button, Cancel (Escape) backing out.
pub fn stop_one_of(
    kind: &'static str,
    name: String,
    forwards: Vec<ForwardSummary>,
    window: &mut Window,
    cx: &mut App,
) {
    match forwards.as_slice() {
        [] => {}
        [only] => stop_forward(only, window, cx),
        _ => {
            let forwards = std::rc::Rc::new(forwards);
            window.open_dialog(cx, move |dialog, window, _cx| {
                use gpui_kit::component::button::{Button, ButtonVariants as _};
                use gpui_kit::component::dialog::{Cancel, DialogFooter};
                use gpui_kit::component::kbd::Kbd;
                let mut footer = DialogFooter::new();
                for (index, forward) in forwards.iter().enumerate() {
                    let forwards = forwards.clone();
                    footer = footer.child(
                        Button::new(pick_button_id(index))
                            .label(format!(
                                "{} \u{2192} {}",
                                forward.local_addr, forward.target_port
                            ))
                            .primary()
                            .on_click(move |_event, window, cx| {
                                window.close_dialog(cx);
                                stop_forward(&forwards[index], window, cx);
                            }),
                    );
                }
                let cancel_key = Kbd::binding_for_action(&Cancel, Some("Dialog"), window);
                dialog
                    .title("Stop Which Port Forward?")
                    .child(format!("{kind} {name} has more than one port-forward."))
                    .footer(
                        footer.child(
                            Button::new(PICK_CANCEL_ID)
                                .label("Cancel")
                                .children(cancel_key)
                                .on_click(|_event, window, cx| window.close_dialog(cx)),
                        ),
                    )
            });
        }
    }
}

#[cfg(test)]
mod tests {
    // Named imports: `super::*` would bring in `gpui_kit::*`'s `test` macro.
    use super::{StopTarget, body};

    #[test]
    fn the_question_names_the_address_and_the_target() {
        let target = |via_service: Option<&str>| StopTarget {
            pod: "web-1".into(),
            via_service: via_service.map(str::to_string),
            local_addr: "127.0.0.1:18080".parse().unwrap(),
            target_port: 8080,
        };
        assert_eq!(
            body(&target(None)).name_list(),
            ["127.0.0.1:18080", "web-1:8080"]
        );
        let via = body(&target(Some("web")));
        assert_eq!(via.name_list(), ["127.0.0.1:18080", "web", "web-1:8080"]);
        assert!(
            via.plain().contains("service \u{201c}web\u{201d}") || via.plain().contains("service "),
            "{}",
            via.plain()
        );
    }
}
