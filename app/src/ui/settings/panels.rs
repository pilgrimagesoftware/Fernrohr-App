//! The Settings window's Panels section (`logs-panel-instancing`): where a
//! pod's logs open - a Logs panel of its own (the default) or the one shared
//! Logs panel. The two buttons dispatch the same commands as the palette
//! (`ui::logs_panels`), so the preference changes at once and is saved. They
//! are tab stops; Enter or Space presses them.

use crate::config::ui::LogsPanels;
use crate::ui::logs_panels::{OpenLogsPerPod, ReuseOneLogsPanel};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// The element id of the button choosing `panels`.
pub(super) fn logs_button_id(panels: LogsPanels) -> &'static str {
    match panels {
        LogsPanels::PerPod => "settings-logs-per-pod",
        LogsPanels::Reuse => "settings-logs-reuse",
    }
}

/// The Logs row: label, then one button per choice, the current one marked.
fn logs_row(cx: &App) -> impl IntoElement {
    let current = crate::ui::logs_panels::current(cx);
    let choice = |panels: LogsPanels, label: &'static str, action: Box<dyn Action>| {
        Button::new(logs_button_id(panels))
            .label(label)
            .xsmall()
            .when(panels == current, |button| button.primary())
            .when(panels != current, |button| button.ghost())
            .on_click(move |_event, window, cx| window.dispatch_action(action.boxed_clone(), cx))
    };
    div()
        .flex()
        .items_center()
        .gap(crate::ui::space::spacing(cx).control_gap)
        .child(div().w(rems(7.5)).child("Pod logs"))
        .child(choice(
            LogsPanels::PerPod,
            "Own panel per pod",
            Box::new(OpenLogsPerPod),
        ))
        .child(choice(
            LogsPanels::Reuse,
            "One shared panel",
            Box::new(ReuseOneLogsPanel),
        ))
}

/// The section: the Logs row, and what `shift-l` does meanwhile.
pub(super) fn section(cx: &App) -> impl IntoElement {
    use gpui_kit::component::ActiveTheme as _;
    let space = crate::ui::space::spacing(cx);
    div()
        .p(space.panel_inset)
        .flex()
        .flex_col()
        .gap(space.control_gap)
        .child(logs_row(cx))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Shift-L opens a pod's logs the other way, once."),
        )
}
