//! The banner an action the cluster refused leaves above a panel's content:
//! what was asked, the server's reason, its full detail, and Dismiss. Shared
//! by every panel that acts on objects - the Pods list, kind lists, detail
//! panels - so a refusal reads alike wherever it happens. A refused action
//! changed nothing, so the panel's content stays as it was beneath it.
//!
//! Dismiss is an icon button with a tooltip, not a text button
//! (`icon-buttons.md`): a tab stop, pressed with Enter or Space.

use crate::k8s::resource::resource_actions::ActionFailure;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::IconName;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::*;

/// The Dismiss button's tooltip: what it does, since it shows only an icon.
pub(crate) const DISMISS_TOOLTIP: &str = "Dismiss";

/// An action the cluster refused: what was asked, and why it was refused.
#[derive(Clone, Debug)]
pub(crate) struct Refusal {
    /// "Delete Secret db-password"
    pub(crate) action: String,
    pub(crate) failure: ActionFailure,
}

/// `refusal` as a banner with the element id `id`; its Dismiss button,
/// `dismiss_id`, runs `on_dismiss`.
pub(crate) fn render(
    refusal: Refusal,
    id: &'static str,
    dismiss_id: &'static str,
    on_dismiss: impl Fn(&mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let Refusal { action, failure } = refusal;
    let space = crate::ui::space::spacing(cx);
    let bad = crate::ui::style::status(crate::ui::style::Tone::Bad, cx);
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex()
        .items_start()
        .gap(space.control_gap)
        .mb(space.control_gap)
        .px(space.panel_inset)
        .py(space.control_gap)
        .rounded_md()
        .border_1()
        .border_color(bad)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_color(bad)
                        .child(format!("{action} failed: {}", failure.message)),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(failure.detail),
                ),
        )
        .child(
            Button::new(dismiss_id)
                .icon(IconName::Close)
                .xsmall()
                .ghost()
                .tooltip(DISMISS_TOOLTIP)
                .on_click(move |_event, _window, cx| on_dismiss(cx)),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests;
