//! The Settings window's building blocks (#251): a section's body, a titled
//! card grouping its controls, and the hint text that explains them.
//!
//! A card sits on the card surface with the theme's border, as the detail
//! panels' do. A hint is muted, smaller than the controls' text, and in the
//! data font, so it reads as an aside to the label and control above it.

use crate::ui::typography::TypeRole as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;

/// A section's body: its cards, stacked inside the panel inset.
pub(super) fn body(cx: &App) -> Div {
    let space = crate::ui::space::spacing(cx);
    div()
        .p(space.panel_inset)
        .flex()
        .flex_col()
        .gap(space.section_gap)
}

/// A card titled `title`, for a group of controls added as its children.
pub(super) fn card(title: &'static str, cx: &App) -> Div {
    let space = crate::ui::space::spacing(cx);
    div()
        .flex()
        .flex_col()
        .gap(space.control_gap)
        .p(space.card_padding)
        .rounded_md()
        .bg(crate::ui::style::surface_card(cx))
        .border_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
}

/// A hint under a setting: muted, smaller, in the data font.
pub(super) fn hint(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .data_font()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

#[cfg(test)]
mod tests;
