//! The Tunnels window's Port forwards section (`k9s-remaining-keybindings`
//! 4.3): every forward started from a Pods or Services row, what it forwards,
//! where it listens and its state, with a Stop that releases it. Stop is a
//! button per row - a tab stop, so Tab and Space reach it as a click does.

use super::*;
use crate::forward::managed::ForwardState;

/// A forward row's Stop button.
pub(super) fn stop_button_id(index: usize) -> SharedString {
    format!("tunnels-port-forward-stop-{index}").into()
}

fn state_label(state: ForwardState) -> &'static str {
    match state {
        ForwardState::Disconnected => "Stopped",
        ForwardState::Connecting => "Connecting",
        ForwardState::Up => "Up",
        ForwardState::Reconnecting => "Reconnecting",
    }
}

impl TunnelsWindow {
    /// The Port forwards section, or nothing while none is running.
    pub(super) fn port_forwards_section(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let forwards = self.forwards.read(cx).list();
        if forwards.is_empty() {
            return None;
        }
        let theme = cx.theme().clone();
        let rows = forwards
            .into_iter()
            .enumerate()
            .map(|(index, (request, addr, state))| {
                let entity = self.forwards.clone();
                let request_for_stop = request.clone();
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .py_1()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_sm().font_medium().child(format!(
                                "{} \u{2192} {}:{}",
                                addr, request.pod, request.remote_port
                            )))
                            .child(div().text_xs().text_color(theme.muted_foreground).child(
                                format!("{} in {}", request.namespace, request.context_name),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(if state == ForwardState::Up {
                                        theme.success
                                    } else {
                                        theme.muted_foreground
                                    })
                                    .child(state_label(state)),
                            )
                            .child(
                                Button::new(stop_button_id(index))
                                    .label("Stop")
                                    .outline()
                                    .xsmall()
                                    .on_click(move |_event, _window, cx| {
                                        entity.update(cx, |forwards, cx| {
                                            forwards.stop(&request_for_stop, cx);
                                        });
                                    }),
                            ),
                    )
            });
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_sm().font_medium().child("Port forwards"))
                .children(rows)
                .into_any_element(),
        )
    }
}
