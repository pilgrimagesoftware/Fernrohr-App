//! Drawing the quick look: the pod at a glance (`pod_detail::glance`), its
//! latest Warning, and the Open Details button with the popover's key hints.

use super::{QuickLookPopover, Warning};
use crate::k8s::resource::pod_detail::glance::{Glance, glance};
use crate::k8s::resource::pods::commands::{
    CLOSE_QUICK_LOOK_KEY, OPEN_QUICK_LOOK_DETAILS_KEY, QUICK_LOOK_KEY_CONTEXT,
};
use crate::k8s::resource::pods::{CloseQuickLook, OpenQuickLookDetails, format_age};
use crate::ui::detail;
use crate::ui::style;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;
use jiff::Timestamp;

/// The popover's element id, and its Open Details button's.
pub(in crate::k8s::resource::pods) const POPOVER_ID: &str = "pod-quick-look";
pub(in crate::k8s::resource::pods) const OPEN_DETAILS_ID: &str = "pod-quick-look-open-details";

impl Render for QuickLookPopover {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let space = crate::ui::space::spacing(cx);
        let now = Timestamp::now();
        let body = match self.pod(cx).map(|pod| glance(pod, now)) {
            Some(glance) => self.render_glance(&glance, now, cx),
            None => div()
                .child(format!("Pod {} no longer exists.", self.target.name))
                .into_any_element(),
        };
        let key = |action: &dyn Action, literal: &str| {
            Kbd::binding_for_action(action, Some(QUICK_LOOK_KEY_CONTEXT), window)
                .unwrap_or_else(|| Kbd::new(Keystroke::parse(literal).expect("valid keybinding")))
        };
        let hints = div()
            .flex()
            .items_center()
            .gap(space.control_gap)
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(
                Button::new(OPEN_DETAILS_ID)
                    .small()
                    .primary()
                    .label("Open Details")
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(OpenQuickLookDetails), cx);
                    }),
            )
            .child(key(&OpenQuickLookDetails, OPEN_QUICK_LOOK_DETAILS_KEY))
            .child(key(&CloseQuickLook, CLOSE_QUICK_LOOK_KEY))
            .child("Close");
        div()
            .id(POPOVER_ID)
            .occlude()
            .w(px(420.))
            .p(space.panel_inset)
            .flex()
            .flex_col()
            .gap(space.control_gap)
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .shadow_lg()
            .child(body)
            .child(hints)
            .test_support()
    }
}

impl QuickLookPopover {
    fn render_glance(&self, glance: &Glance, now: Timestamp, cx: &Context<Self>) -> AnyElement {
        let mut rows = vec![
            detail::row(
                "Status",
                div()
                    .debug_selector(|| "quick-look-status".into())
                    .text_color(style::status(glance.status_tone, cx))
                    .child(glance.status.clone()),
                cx,
            ),
            detail::row(
                "Ready",
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .size(px(7.))
                            .rounded_full()
                            .bg(style::status(glance.ready_tone, cx)),
                    )
                    .child(glance.ready.clone()),
                cx,
            ),
            detail::row("Restarts", glance.restarts.to_string(), cx),
            detail::row("Age", glance.age.clone(), cx),
            detail::row("Node", glance.node.clone(), cx),
            detail::row("Pod IP", glance.pod_ip.clone(), cx),
        ];
        if !glance.owners.is_empty() {
            rows.push(detail::row(
                "Controlled By",
                crate::ui::link::references(
                    "quick-look-owner",
                    &glance.owners,
                    |owner| owner.qualified_name(),
                    &self.target.context_name,
                    self.discovery.read(cx).kinds(),
                    cx,
                ),
                cx,
            ));
        }
        for container in &glance.containers {
            rows.push(detail::row(
                container.name.clone(),
                div()
                    .flex()
                    .flex_wrap()
                    .gap_x_2()
                    .child(container.image.clone())
                    .child(
                        div()
                            .text_color(detail::tone_color(container.state_tone, cx))
                            .child(container.state.clone()),
                    ),
                cx,
            ));
        }
        if let Some(warning) = self.latest_warning(cx) {
            rows.push(detail::row(
                "Warning",
                div()
                    .debug_selector(|| "quick-look-warning".into())
                    .text_color(style::status(style::Tone::Warning, cx))
                    .child(warning_text(&warning, now)),
                cx,
            ));
        }
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap_2()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(glance.name.clone()),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(glance.namespace.clone()),
                    ),
            )
            .child(detail::striped(rows, cx))
            .into_any_element()
    }
}

/// `BackOff: Back-off restarting failed container (2m ago)`.
pub(super) fn warning_text(warning: &Warning, now: Timestamp) -> String {
    match warning.last_seen {
        Some(seen) => format!(
            "{}: {} ({} ago)",
            warning.reason,
            warning.message,
            format_age(now.duration_since(seen).as_secs())
        ),
        None => format!("{}: {}", warning.reason, warning.message),
    }
}
