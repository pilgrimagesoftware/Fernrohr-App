//! The Events tab's time-window commands (`pod-events-time-window` 2.2), scoped to
//! the pod detail panel: `[` and `]` step the window shorter and longer, and one
//! palette command per window picks it outright. Each changes this panel's window
//! and saves it as the preference.

use super::PodDetailPanel;
use super::commands::PANEL_KEY_CONTEXT;
use crate::command::{Command, CommandRegistry};
use crate::config::ui::PodEventsWindow;
use gpui_kit::*;

actions!(
    pod_detail,
    [
        ShorterEventsWindow,
        LongerEventsWindow,
        EventsWindow15Minutes,
        EventsWindow1Hour,
        EventsWindow6Hours,
        EventsWindow24Hours,
        EventsWindowAll
    ]
);

pub(crate) const SHORTER_KEY: &str = "[";
pub(crate) const LONGER_KEY: &str = "]";

/// The window each pick-outright action selects.
pub(super) fn action_for(window: PodEventsWindow) -> Box<dyn Action> {
    match window {
        PodEventsWindow::Minutes15 => Box::new(EventsWindow15Minutes),
        PodEventsWindow::Hour1 => Box::new(EventsWindow1Hour),
        PodEventsWindow::Hours6 => Box::new(EventsWindow6Hours),
        PodEventsWindow::Hours24 => Box::new(EventsWindow24Hours),
        PodEventsWindow::All => Box::new(EventsWindowAll),
    }
}

pub(super) fn register_commands(registry: &mut CommandRegistry) {
    let mut register = |id, title, default_binding, action: Box<dyn Action>| {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: Some(PANEL_KEY_CONTEXT),
            action,
            menu: None,
        });
    };
    register(
        "pod_detail.events_window_shorter",
        "Pod Detail: Events Window - Shorter",
        SHORTER_KEY,
        Box::new(ShorterEventsWindow),
    );
    register(
        "pod_detail.events_window_longer",
        "Pod Detail: Events Window - Longer",
        LONGER_KEY,
        Box::new(LongerEventsWindow),
    );
    for (id, title, window) in [
        (
            "pod_detail.events_window_15m",
            "Pod Detail: Events Window - 15 minutes",
            PodEventsWindow::Minutes15,
        ),
        (
            "pod_detail.events_window_1h",
            "Pod Detail: Events Window - 1 hour",
            PodEventsWindow::Hour1,
        ),
        (
            "pod_detail.events_window_6h",
            "Pod Detail: Events Window - 6 hours",
            PodEventsWindow::Hours6,
        ),
        (
            "pod_detail.events_window_24h",
            "Pod Detail: Events Window - 24 hours",
            PodEventsWindow::Hours24,
        ),
        (
            "pod_detail.events_window_all",
            "Pod Detail: Events Window - All",
            PodEventsWindow::All,
        ),
    ] {
        register(id, title, "", action_for(window));
    }
}

impl PodDetailPanel {
    /// Makes `window` this panel's, and the preference new panels start from.
    pub(super) fn set_events_window(&mut self, window: PodEventsWindow, cx: &mut Context<Self>) {
        self.events_window = window;
        super::window_preference::set_preferred(window, cx);
        cx.notify();
    }

    /// The panel root's listeners for every window action.
    pub(super) fn with_window_actions(element: Div, cx: &mut Context<Self>) -> Div {
        let pick = |window: PodEventsWindow| {
            move |this: &mut Self, window_: &mut Window, cx: &mut Context<Self>| {
                let _ = window_;
                this.set_events_window(window, cx)
            }
        };
        let (m15, h1, h6, h24, all) = (
            pick(PodEventsWindow::Minutes15),
            pick(PodEventsWindow::Hour1),
            pick(PodEventsWindow::Hours6),
            pick(PodEventsWindow::Hours24),
            pick(PodEventsWindow::All),
        );
        element
            .on_action(cx.listener(|this, _: &ShorterEventsWindow, _, cx| {
                this.set_events_window(this.events_window.shorter(), cx)
            }))
            .on_action(cx.listener(|this, _: &LongerEventsWindow, _, cx| {
                this.set_events_window(this.events_window.longer(), cx)
            }))
            .on_action(cx.listener(move |this, _: &EventsWindow15Minutes, w, cx| m15(this, w, cx)))
            .on_action(cx.listener(move |this, _: &EventsWindow1Hour, w, cx| h1(this, w, cx)))
            .on_action(cx.listener(move |this, _: &EventsWindow6Hours, w, cx| h6(this, w, cx)))
            .on_action(cx.listener(move |this, _: &EventsWindow24Hours, w, cx| h24(this, w, cx)))
            .on_action(cx.listener(move |this, _: &EventsWindowAll, w, cx| all(this, w, cx)))
    }
}
