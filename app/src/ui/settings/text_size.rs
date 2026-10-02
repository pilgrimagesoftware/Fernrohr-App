//! The Settings window's Text Size row: the text-size preference's value with
//! a stepper. Its buttons dispatch the same commands as cmd-= / cmd-- /
//! cmd-shift-0 (`ui::text_size`), so the keys, the menu, the palette and these
//! buttons all take one path. The buttons are tab stops, and Enter or Space
//! presses them.

use crate::ui::text_size::{self, DecreaseTextSize, IncreaseTextSize, ResetTextSize};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, kbd::Kbd};
use gpui_kit::*;

/// The row's key context: no bindings of its own, it marks where focus is.
pub(super) const KEY_CONTEXT: &str = "TextSizeStepper";

/// The row: label, −, value, +, Reset, and the three keys. Its widths are
/// rems, so they grow with the text they hold.
pub(super) fn row(window: &mut Window, cx: &mut App) -> impl IntoElement {
    let size = text_size::current(cx);
    let muted = cx.theme().muted_foreground;
    let step = |id: &'static str, label: &'static str, action: Box<dyn Action>| {
        Button::new(id)
            .label(label)
            .xsmall()
            .on_click(move |_event, window, cx| window.dispatch_action(action.boxed_clone(), cx))
    };
    let keys = [
        Kbd::binding_for_action(&DecreaseTextSize, None, window),
        Kbd::binding_for_action(&IncreaseTextSize, None, window),
        Kbd::binding_for_action(&ResetTextSize, None, window),
    ];
    div()
        .key_context(KEY_CONTEXT)
        .flex()
        .items_center()
        .gap(crate::ui::space::spacing(cx).control_gap)
        .child(div().w(rems(7.5)).child("Text Size"))
        .child(step("text-size-decrease", "−", Box::new(DecreaseTextSize)))
        .child(
            div()
                .debug_selector(|| "text-size-value".into())
                .w(rems(3.5))
                .text_center()
                .child(format!("{}%", size.percent())),
        )
        .child(step("text-size-increase", "+", Box::new(IncreaseTextSize)))
        .child(step("text-size-reset", "Reset", Box::new(ResetTextSize)).ghost())
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .text_xs()
                .text_color(muted)
                .children(keys.into_iter().flatten()),
        )
}

#[cfg(test)]
mod tests;
