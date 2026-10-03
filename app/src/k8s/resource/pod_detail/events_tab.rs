//! Drawing the pod's Events tab (`pod-events-time-window` 2.1-2.2): the window's
//! events, then how many older ones it hides - an empty list says it's empty *in
//! the window*, so it's never read as "no events at all". And the Overview tab's
//! recent warnings within the same window (3.1), linking to the Events tab.

use super::PodDetailPanel;
use super::commands::PANEL_KEY_CONTEXT;
use super::live_events::EventsView;
use super::model::DetailSection;
use super::window_commands::{
    LONGER_KEY, LongerEventsWindow, SHORTER_KEY, ShorterEventsWindow, action_for,
};
use crate::config::ui::PodEventsWindow;
use crate::k8s::resource::events::EventSummary;
use crate::ui::detail::BadgeTone;
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

/// How many recent warnings the Overview tab shows.
const OVERVIEW_WARNINGS: usize = 3;

impl PodDetailPanel {
    /// The pod's most recent Warning events within the window, newest first, at
    /// most [`OVERVIEW_WARNINGS`] - what the Overview tab surfaces (3.1).
    pub(super) fn overview_warnings(&self, now: Timestamp, cx: &App) -> Vec<EventSummary> {
        let Some(EventsView {
            events: Ok(events), ..
        }) = self.events_view(now, cx)
        else {
            return Vec::new();
        };
        events
            .into_iter()
            .filter(|event| event.tone == BadgeTone::Warning)
            .take(OVERVIEW_WARNINGS)
            .collect()
    }

    /// The Overview's warnings block - nothing at all when there are none - with
    /// a link to the Events tab, which opens it as clicking its tab (or `5`) does.
    pub(super) fn render_overview_warnings(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let warnings = self.overview_warnings(Timestamp::now(), cx);
        if warnings.is_empty() {
            return None;
        }
        let this = cx.weak_entity();
        let heading = match self.events_window {
            PodEventsWindow::All => "Recent warnings".to_string(),
            window => format!("Recent warnings (last {})", window.label()),
        };
        Some(
            div()
                .debug_selector(|| "pod-overview-warnings".into())
                .flex()
                .flex_col()
                .gap(crate::ui::space::spacing(cx).control_gap)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_sm().child(heading))
                        .child(
                            Button::new("pod-overview-events-link")
                                .label("All events")
                                .link()
                                .xsmall()
                                .on_click(move |_event, _window, cx| {
                                    let _ = this.update(cx, |panel, cx| {
                                        panel.set_active_tab(DetailSection::Events, cx)
                                    });
                                }),
                        ),
                )
                .child(crate::ui::detail::events(&Ok(warnings), cx))
                .into_any_element(),
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
