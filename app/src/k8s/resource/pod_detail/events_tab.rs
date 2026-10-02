//! Drawing the pod's Events tab (`pod-events-time-window` 2.1-2.2): the window's
//! events, then how many older ones it hides - an empty list says it's empty *in
//! the window*, so it's never read as "no events at all".

use super::PodDetailPanel;
use super::commands::PANEL_KEY_CONTEXT;
use super::window_commands::{
    LONGER_KEY, LongerEventsWindow, SHORTER_KEY, ShorterEventsWindow, action_for,
};
use crate::config::ui::PodEventsWindow;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;
use jiff::Timestamp;

impl PodDetailPanel {
    pub(super) fn render_events_view(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        let (events, hidden) = match self.events_view(Timestamp::now(), cx) {
            Some(view) => (view.events, view.hidden),
            None => (Ok(Vec::new()), 0),
        };
        let muted = cx.theme().muted_foreground;
        let list = match &events {
            Ok(list) if list.is_empty() => div()
                .text_sm()
                .text_color(muted)
                .child(empty_text(self.events_window))
                .into_any_element(),
            _ => crate::ui::detail::events(&events, cx),
        };
        div()
            .flex()
            .flex_col()
            .gap(crate::ui::space::spacing(cx).control_gap)
            .child(self.window_selector(window, cx))
            .child(list)
            .children((hidden > 0).then(|| {
                div()
                    .debug_selector(|| "pod-events-hidden".into())
                    .text_xs()
                    .text_color(muted)
                    .child(hidden_text(hidden))
            }))
            .into_any_element()
    }
}

impl PodDetailPanel {
    /// The window selector: one button per window, the current one filled, then
    /// the `[` / `]` hint. Each button dispatches its window's command.
    fn window_selector(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let current = self.events_window;
        div()
            .flex()
            .items_center()
            .gap_1()
            .children(PodEventsWindow::ALL.into_iter().map(move |window| {
                let id = SharedString::from(format!("pod-events-window-{}", window.label()));
                let button = Button::new(id).label(window.label()).xsmall();
                let button = if window == current {
                    button.primary()
                } else {
                    button.ghost()
                };
                button.on_click(move |_event, window_, cx| {
                    window_.dispatch_action(action_for(window), cx)
                })
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(key(&ShorterEventsWindow, SHORTER_KEY, window))
                    .child(key(&LongerEventsWindow, LONGER_KEY, window))
                    .child("Window"),
            )
    }
}

/// `action`'s key from the live keymap, falling back to its default.
fn key(action: &dyn Action, default: &str, window: &Window) -> Kbd {
    Kbd::binding_for_action(action, Some(PANEL_KEY_CONTEXT), window)
        .unwrap_or_else(|| Kbd::new(Keystroke::parse(default).expect("valid keybinding")))
}

/// What an empty Events tab says for `window`.
pub(super) fn empty_text(window: PodEventsWindow) -> String {
    match window {
        PodEventsWindow::All => "No events.".to_string(),
        window => format!("No events in the last {}.", window.label()),
    }
}

/// The line under the list when the window hides some.
pub(super) fn hidden_text(hidden: usize) -> String {
    if hidden == 1 {
        "1 older event hidden".to_string()
    } else {
        format!("{hidden} older events hidden")
    }
}
