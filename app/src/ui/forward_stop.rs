//! The one confirmation every way of stopping a port-forward goes through
//! (`port-forward-indicators` 5.2): the app's shared confirmation
//! (`ui::confirm_dialog`), naming the pod or Service, where the forward listens
//! and the port it reaches, and stopping it only once confirmed. Enter or Stop
//! Forward stops it; Escape or Cancel keeps it.
//!
//! [`stop_forward`] is that whole path for one forward.

use crate::k8s::cluster::port_forwards::{ForwardSummary, PortForwards};
use crate::ui::confirm_dialog::{self, Confirmation};
use crate::ui::confirm_text::ConfirmText;
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

/// What is being stopped: `kind` and `name` of the object it forwards to, where
/// it listens, and the port it reaches.
pub struct StopTarget {
    pub kind: &'static str,
    pub name: String,
    pub local_addr: SocketAddr,
    pub target_port: u16,
}

/// The question, the names set apart.
fn body(target: &StopTarget) -> ConfirmText {
    ConfirmText::new()
        .text("Stop forwarding ")
        .name(&target.local_addr.to_string())
        .text(&format!(" to {} ", target.kind))
        .name(&format!("{}:{}", target.name, target.target_port))
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
    };
    confirm_dialog::open(confirmation, stop, window, cx);
}

/// Asks, then stops `forward` - one of `kind` `name`'s.
pub fn stop_forward(
    kind: &'static str,
    name: &str,
    forward: &ForwardSummary,
    window: &mut Window,
    cx: &mut App,
) {
    let target = StopTarget {
        kind,
        name: name.to_string(),
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

#[cfg(test)]
mod tests {
    // Named imports: `super::*` would bring in `gpui_kit::*`'s `test` macro.
    use super::{StopTarget, body};

    #[test]
    fn the_question_names_the_address_and_the_target() {
        let text = body(&StopTarget {
            kind: "pod",
            name: "web-1".into(),
            local_addr: "127.0.0.1:18080".parse().unwrap(),
            target_port: 8080,
        });
        assert_eq!(text.name_list(), ["127.0.0.1:18080", "web-1:8080"]);
    }
}
