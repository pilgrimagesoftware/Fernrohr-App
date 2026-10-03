//! Drawing the quick look: the pod at a glance (`pod_detail::glance`), its
//! latest Warning, and a footer pairing each key with its action.
//!
//! The popover is as wide as its fields need, between
//! `QUICK_LOOK_MIN_WIDTH` and a share of the window (capped). A value longer
//! than that - a cloud node name, a fully qualified image - stays on one line,
//! ellipsized, with its full text in a hover tooltip and a copy control beside
//! it, as the detail panels show a long value.

use super::{QuickLookPopover, Warning};
use crate::consts::{QUICK_LOOK_MAX_WIDTH, QUICK_LOOK_MAX_WIDTH_FRACTION, QUICK_LOOK_MIN_WIDTH};
use crate::k8s::resource::pod_detail::glance::{Glance, glance};
use crate::k8s::resource::pods::commands::{
    CLOSE_QUICK_LOOK_KEY, OPEN_QUICK_LOOK_DETAILS_KEY, QUICK_LOOK_KEY_CONTEXT,
};
use crate::k8s::resource::pods::{CloseQuickLook, OpenQuickLookDetails, format_age};
use crate::ui::detail;
use crate::ui::style;
use crate::ui::typography::TypeRole as _;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;
use jiff::Timestamp;

/// The popover's element id, and its Open Details button's.
pub(in crate::k8s::resource::pods) const POPOVER_ID: &str = "pod-quick-look";
pub(in crate::k8s::resource::pods) const OPEN_DETAILS_ID: &str = "pod-quick-look-open-details";

/// The debug selector of the value named `key` - `name`, `node`, `image app`.
pub(in crate::k8s::resource::pods) fn value_selector(key: &str) -> String {
    format!("quick-look-value {key}")
}

/// The debug selectors of the footer's two hints, and of each one's key and label.
pub(in crate::k8s::resource::pods) const OPEN_HINT: &str = "quick-look-hint open";
pub(in crate::k8s::resource::pods) const CLOSE_HINT: &str = "quick-look-hint close";

/// The width of the field labels' column - narrower than the detail panels',
/// since a popover is.
const LABEL_WIDTH: f32 = 104.;

impl Render for QuickLookPopover {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let space = crate::ui::space::spacing(cx);
        let now = Timestamp::now();
        let content = match self.pod().map(|pod| glance(pod, now)) {
            Some(glance) => self.render_glance(&glance, now, cx),
            None => div()
                .child(format!("Pod {} isn't listed.", self.target.name))
                .into_any_element(),
        };
        // Terminating, or deleted with its last state kept and dimmed - the
        // detail panels' own banner and words.
        let body =
            crate::ui::detail::lifecycle::body(content, self.lifecycle().as_ref(), "pod", cx);
        let (min_width, max_width) = width_bounds(window.viewport_size().width);
        div()
            .id(POPOVER_ID)
            .occlude()
            .min_w(min_width)
            .max_w(max_width)
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
            .child(self.render_footer(window, cx))
            .test_support()
    }
}

/// The popover's least and greatest width in a window `window_width` wide.
fn width_bounds(window_width: Pixels) -> (Pixels, Pixels) {
    let max = px(QUICK_LOOK_MAX_WIDTH).min(window_width * QUICK_LOOK_MAX_WIDTH_FRACTION);
    (px(QUICK_LOOK_MIN_WIDTH).min(max), max)
}

impl QuickLookPopover {
    /// "↵ Open Details" and "Esc Close": each key beside its own action, the two
    /// set apart, keys read from the live keymap. Escape is the close key shown;
    /// Space closes too, as it opened.
    fn render_footer(&self, window: &Window, cx: &App) -> impl IntoElement {
        let key = |action: &dyn Action, literal: &str| {
            Kbd::binding_for_action(action, Some(QUICK_LOOK_KEY_CONTEXT), window)
                .unwrap_or_else(|| Kbd::new(Keystroke::parse(literal).expect("valid keybinding")))
        };
        let hint = |selector: &'static str, key: Kbd, label: AnyElement| {
            div()
                .debug_selector(move || selector.into())
                .flex()
                .flex_none()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .debug_selector(move || format!("{selector} key"))
                        .child(key),
                )
                .child(
                    div()
                        .debug_selector(move || format!("{selector} label"))
                        .child(label),
                )
        };
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x_4()
            .gap_y_1()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(hint(
                OPEN_HINT,
                key(&OpenQuickLookDetails, OPEN_QUICK_LOOK_DETAILS_KEY),
                Button::new(OPEN_DETAILS_ID)
                    .small()
                    .primary()
                    .label("Open Details")
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(OpenQuickLookDetails), cx);
                    })
                    .into_any_element(),
            ))
            .child(hint(
                CLOSE_HINT,
                key(&CloseQuickLook, CLOSE_QUICK_LOOK_KEY),
                "Close".into_any_element(),
            ))
    }

    fn render_glance(&self, glance: &Glance, now: Timestamp, cx: &Context<Self>) -> AnyElement {
        let mut rows = vec![
            row(
                "Status",
                div()
                    .debug_selector(|| "quick-look-status".into())
                    .text_color(style::status(glance.status_tone, cx))
                    .child(glance.status.clone()),
                cx,
            ),
            row(
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
            row("Restarts", glance.restarts.to_string(), cx),
            row("Age", glance.age.clone(), cx),
            row("Node", long_value("node", glance.node.clone()), cx),
            row("Pod IP", long_value("ip", glance.pod_ip.clone()), cx),
        ];
        if !glance.owners.is_empty() {
            rows.push(row(
                "Controlled By",
                div()
                    .debug_selector(|| value_selector("owner"))
                    .min_w_0()
                    .overflow_hidden()
                    .child(crate::ui::link::references(
                        "quick-look-owner",
                        &glance.owners,
                        |owner| owner.qualified_name(),
                        &self.target.context_name,
                        self.discovery.read(cx).kinds(),
                        cx,
                    )),
                cx,
            ));
        }
        for container in &glance.containers {
            rows.push(row(
                container.name.clone(),
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(long_value(
                        &format!("image {}", container.name),
                        container.image.clone(),
                    ))
                    .child(
                        div()
                            .debug_selector({
                                let key = format!("state {}", container.name);
                                move || value_selector(&key)
                            })
                            .min_w_0()
                            .truncate()
                            .text_color(detail::tone_color(container.state_tone, cx))
                            .child(container.state.clone()),
                    ),
                cx,
            ));
        }
        if let Some(warning) = self.latest_warning(cx) {
            // Wrapped rather than cut: the warning is often the reason the
            // user looked, and it fits the popover's width by wrapping.
            rows.push(row(
                "Warning",
                div()
                    .debug_selector(|| "quick-look-warning".into())
                    .min_w_0()
                    .whitespace_normal()
                    .text_color(style::status(style::Tone::Warning, cx))
                    .child(warning_text(&warning, now)),
                cx,
            ));
        }
        div()
            .flex()
            .flex_col()
            .gap_1()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap_2()
                    .min_w_0()
                    .child(
                        div()
                            .min_w_0()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(long_value("name", glance.name.clone())),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(glance.namespace.clone()),
                    ),
            )
            .child(detail::striped(rows, cx))
            .into_any_element()
    }
}

/// One field: a label in the popover's narrow column, and its value, which may
/// shrink below its own width so a long one ellipsizes instead of widening the
/// popover past its cap.
fn row(label: impl Into<SharedString>, value: impl IntoElement, cx: &App) -> AnyElement {
    div()
        .data_font()
        .flex()
        .gap_3()
        .py_1()
        .px_2()
        .rounded_sm()
        .child(
            div()
                .w(px(LABEL_WIDTH))
                .flex_none()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(label.into()),
        )
        .child(div().flex_1().min_w_0().child(value))
        .into_any_element()
}

/// `text` on one line, ellipsized if it's longer than there is room for, its
/// full text in a tooltip and copyable from the control beside it. `key`
/// names it for its ids and debug selector.
fn long_value(key: &str, text: String) -> AnyElement {
    let selector = value_selector(key);
    let full = text.clone();
    let content = div()
        .id(SharedString::from(format!("quick-look-text {key}")))
        .debug_selector(move || selector)
        .flex_1()
        .min_w_0()
        .truncate()
        .tooltip(move |window, cx| Tooltip::new(full.clone()).build(window, cx))
        .child(text.clone());
    crate::ui::copy::copyable(
        content,
        SharedString::from(format!("quick-look-copy {key}")),
        text,
        format!("quick-look-value {key}"),
    )
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
