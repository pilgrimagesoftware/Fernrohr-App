//! The pod's port-forwards in its detail panel (`port-forward-indicators` 3-4):
//! a strip above the tabs listing each forward's local address and target port,
//! with copy and stop icons; beside each container port in the Containers tab, a
//! start icon - or, once forwarded, the address with copy and stop icons; and
//! Stop Port Forward, which asks which forward when there are several.
//!
//! Every control is an icon button with a tooltip (`ui::icon_tooltip`), a tab
//! stop that Enter or Space presses, and every stop asks first
//! (`ui::forward_stop`). Starting needs no prompt: the port is the one clicked.

use super::panel::PodDetailPanel;
use crate::k8s::cluster::port_forwards::{ForwardObject, ForwardSummary, PortForwards};
use crate::k8s::resource::pods::StopPortForward;
use crate::ui::icon_tooltip::with_tooltip;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _};
use gpui_kit::*;

/// The strip, and its controls' ids, for tests.
pub(super) const STRIP_ID: &str = "pod-forward-strip";
pub(super) fn strip_copy_id(index: usize) -> SharedString {
    format!("pod-forward-copy-{index}").into()
}
pub(super) fn strip_stop_id(index: usize) -> SharedString {
    format!("pod-forward-stop-{index}").into()
}
pub(super) fn port_start_id(container: &str, port: u16) -> SharedString {
    format!("container-port-forward-{container}-{port}").into()
}
pub(super) fn port_copy_id(container: &str, port: u16) -> SharedString {
    format!("container-port-copy-{container}-{port}").into()
}
pub(super) fn port_stop_id(container: &str, port: u16) -> SharedString {
    format!("container-port-stop-{container}-{port}").into()
}

/// The tooltips.
pub(super) const COPY_TOOLTIP: &str = "Copy address";
pub(super) const STOP_TOOLTIP: &str = "Stop port-forward";
pub(super) const START_TOOLTIP: &str = "Forward this port";

/// A container port's number, from its display string (`8080` or `8080/TCP`).
fn port_number(label: &str) -> Option<u16> {
    label.split('/').next()?.trim().parse().ok()
}

/// An icon button with its tooltip, its wrapper's id `<id>-tooltip`.
fn icon_button(
    id: SharedString,
    icon: IconName,
    tooltip: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    with_tooltip(
        SharedString::from(format!("{id}-tooltip")),
        tooltip,
        Button::new(id)
            .icon(icon)
            .ghost()
            .xsmall()
            .on_click(on_click),
    )
}

impl PodDetailPanel {
    fn forward_object(&self) -> ForwardObject {
        ForwardObject::pod(
            &self.scope.context_name,
            &self.pod.namespace,
            &self.pod.name,
        )
    }

    /// The forwards reaching this panel's pod.
    pub(super) fn pod_forwards(&self, cx: &App) -> Vec<ForwardSummary> {
        PortForwards::existing(cx)
            .map(|forwards| forwards.read(cx).for_object(&self.forward_object()))
            .unwrap_or_default()
    }

    /// Copy and stop icons for `forward`, with these ids.
    fn forward_controls(
        &self,
        forward: &ForwardSummary,
        copy_id: SharedString,
        stop_id: SharedString,
    ) -> [Stateful<Div>; 2] {
        let address = forward.local_addr.to_string();
        let stopping = forward.clone();
        [
            icon_button(copy_id, IconName::Copy, COPY_TOOLTIP, move |_, _, cx| {
                crate::ui::copy::copy_text(&address, cx)
            }),
            icon_button(
                stop_id,
                IconName::CircleX,
                STOP_TOOLTIP,
                move |_, window, cx| crate::ui::forward_stop::stop_forward(&stopping, window, cx),
            ),
        ]
    }

    /// The strip above the tabs while the pod has forwards: each forward's local
    /// address and target port, with copy and stop icons. No border or ring.
    pub(super) fn render_forward_strip(&self, cx: &App) -> Option<AnyElement> {
        let forwards = self.pod_forwards(cx);
        if forwards.is_empty() {
            return None;
        }
        let space = crate::ui::space::spacing(cx);
        let entries = forwards.iter().enumerate().map(|(index, forward)| {
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(format!(
                    "{} \u{2192} {}",
                    forward.local_addr, forward.target_port
                ))
                .children(self.forward_controls(
                    forward,
                    strip_copy_id(index),
                    strip_stop_id(index),
                ))
        });
        Some(
            div()
                .debug_selector(|| STRIP_ID.into())
                .flex()
                .flex_wrap()
                .items_center()
                .gap(space.control_gap)
                .px(space.panel_inset)
                .py(space.control_gap)
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(Icon::new(IconName::ArrowLeftRight).xsmall())
                .children(entries)
                .into_any_element(),
        )
    }

    /// `container`'s ports, each with its forward controls: a start icon, or the
    /// local address with copy and stop icons once it's forwarded.
    pub(super) fn render_ports(&self, container: &str, ports: &[String], cx: &App) -> AnyElement {
        let forwards = self.pod_forwards(cx);
        let chips = ports.iter().map(|label| {
            let chip = div().flex().items_center().gap_1().child(label.clone());
            let Some(port) = port_number(label) else {
                return chip;
            };
            match forwards.iter().find(|forward| forward.target_port == port) {
                Some(forward) => chip
                    .child(format!("\u{2192} {}", forward.local_addr))
                    .children(self.forward_controls(
                        forward,
                        port_copy_id(container, port),
                        port_stop_id(container, port),
                    )),
                None => {
                    let selection = self.selection();
                    chip.child(icon_button(
                        port_start_id(container, port),
                        IconName::ArrowLeftRight,
                        START_TOOLTIP,
                        move |_, window, cx| {
                            crate::k8s::resource::pods::forward_port(&selection, port, window, cx)
                        },
                    ))
                }
            }
        });
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x_2()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child("Ports:")
            .children(chips)
            .into_any_element()
    }

    /// `StopPortForward`: stops one of the pod's forwards - asking which when it
    /// has several - once confirmed.
    pub(super) fn on_action_stop_port_forward(
        &mut self,
        _: &StopPortForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let forwards = self.pod_forwards(cx);
        let name = self.pod.name.clone();
        crate::ui::forward_stop::stop_one_of("pod", name, forwards, window, cx);
    }
}
