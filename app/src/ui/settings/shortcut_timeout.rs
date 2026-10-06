//! The Settings window's Shortcut Timeout row: the preference's value with a
//! stepper. Its buttons dispatch the same commands the palette offers
//! (`ui::shortcut_timeout`), so both take one path. The buttons are tab stops,
//! and Enter or Space presses them.

use crate::ui::shortcut_timeout::{
    self, DecreaseShortcutTimeout, IncreaseShortcutTimeout, ResetShortcutTimeout,
};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::*;

/// The row's key context: no bindings of its own, it marks where focus is.
pub(super) const KEY_CONTEXT: &str = "ShortcutTimeoutStepper";

/// The row: label, −, value, +, Reset. Widths in rems, as the Text Size row's.
pub(super) fn row(cx: &mut App) -> impl IntoElement {
    let timeout = shortcut_timeout::current(cx);
    let step = |id: &'static str, label: &'static str, action: Box<dyn Action>| {
        Button::new(id)
            .label(label)
            .xsmall()
            .on_click(move |_event, window, cx| window.dispatch_action(action.boxed_clone(), cx))
    };
    div()
        .key_context(KEY_CONTEXT)
        .flex()
        .items_center()
        .gap(crate::ui::space::spacing(cx).control_gap)
        .child(div().w(rems(7.5)).child("Shortcut Timeout"))
        .child(step(
            "shortcut-timeout-decrease",
            "−",
            Box::new(DecreaseShortcutTimeout),
        ))
        .child(
            div()
                .debug_selector(|| "shortcut-timeout-value".into())
                .w(rems(3.5))
                .text_center()
                .child(format!("{} s", timeout.secs())),
        )
        .child(step(
            "shortcut-timeout-increase",
            "+",
            Box::new(IncreaseShortcutTimeout),
        ))
        .child(
            step(
                "shortcut-timeout-reset",
                "Reset",
                Box::new(ResetShortcutTimeout),
            )
            .ghost(),
        )
}

#[cfg(test)]
mod tests;
