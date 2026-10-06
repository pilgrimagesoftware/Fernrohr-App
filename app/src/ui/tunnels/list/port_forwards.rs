//! The Tunnels window's Port forwards section (`k9s-remaining-keybindings`
//! 4.3): every forward started from a Pods or Services row, what it forwards,
//! where it listens and its state, with a Stop that releases it. Stop is an
//! icon button per row with a tooltip - a tab stop, so Tab and Space reach it as
//! a click does.
//!
//! `port-forward-indicators` 5: the section sits below a divider from the
//! Tunnels list, shown even with no forward running, and Stop asks first through
//! the shared confirmation (`ui::forward_stop`).

use super::*;
use crate::forward::managed::ForwardState;
use crate::k8s::cluster::port_forwards::ForwardSummary;

/// The divider between the Tunnels list and this section.
pub(super) const DIVIDER_ID: &str = "tunnels-port-forwards-divider";

/// A forward row's Stop button's tooltip.
pub(super) const STOP_TOOLTIP: &str = "Stop port-forward";

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
    /// The Port forwards section, below its divider - saying so when none is
    /// running.
    pub(super) fn port_forwards_section(&self, cx: &Context<Self>) -> AnyElement {
        let forwards = self.forwards.read(cx).list();
        let theme = cx.theme().clone();
        let none = forwards.is_empty().then(|| {
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("None running.")
        });
        let rows = forwards
            .into_iter()
            .enumerate()
            .map(|(index, (request, addr, state))| {
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
                            .child(crate::ui::icon_tooltip::with_tooltip(
                                SharedString::from(format!("{}-tooltip", stop_button_id(index))),
                                STOP_TOOLTIP,
                                Button::new(stop_button_id(index))
                                    .icon(IconName::CircleX)
                                    .ghost()
                                    .xsmall()
                                    .on_click(move |_event, window, cx| {
                                        let forward = ForwardSummary {
                                            request: request_for_stop.clone(),
                                            local_addr: addr,
                                            target_port: request_for_stop.remote_port,
                                            state,
                                        };
                                        crate::ui::forward_stop::stop_forward(&forward, window, cx);
                                    }),
                            )),
                    )
            });
        // The app's section separator: a top border with the section gap above
        // the content, as the editor pane below has.
        div()
            .debug_selector(|| DIVIDER_ID.into())
            .flex()
            .flex_col()
            .gap_1()
            .border_t_1()
            .border_color(theme.border)
            .pt(crate::ui::space::spacing(cx).section_gap)
            .child(div().text_sm().font_medium().child("Port forwards"))
            .children(none)
            .children(rows)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
