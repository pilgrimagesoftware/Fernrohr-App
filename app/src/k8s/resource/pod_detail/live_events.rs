//! The pod's events, live (`pod-events-time-window` 1.2): once the pod has loaded,
//! the panel runs a field-selected Event watch on it (`events::watch`), kept in the
//! events browser's table so both render events alike. The watch is the panel's:
//! it starts when the pod first loads, restarts only if the pod was recreated
//! under the same name (a new uid), and ends when the panel is dropped.
//!
//! The tab shows only events last seen within the panel's time window (2.1, a
//! client-side filter - the API can't select by time), and says how many it
//! hides. A minute's tick re-renders the panel, so events age out while it's open.

use super::PodDetailPanel;
use super::format::non_empty;
use crate::config::ui::PodEventsWindow;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::events::{self, EventSummary, InvolvedObject};
use crate::k8s::resource::events_browser::{EventRow, EventsTable};
use gpui_kit::*;
use jiff::Timestamp;
use k8s_openapi::api::core::v1::Pod;

/// A running watch on one pod's events: which pod (by uid), its table, and the
/// task that keeps the table current - dropping it ends the watch.
pub(super) struct PodEventsWatch {
    uid: String,
    pub(super) table: Entity<EventsTable>,
    _task: Task<()>,
    /// Re-renders the panel every `POD_EVENTS_WINDOW_TICK`, so the window's
    /// edge moves on.
    _tick: Task<()>,
}

/// The Events tab's content: the events within the window, newest first - or why
/// they can't be listed - and how many older ones the window hides.
pub(super) struct EventsView {
    pub(super) events: Result<Vec<EventSummary>, String>,
    pub(super) hidden: usize,
}

/// `rows` last seen within `window` of `now`, and how many it leaves out. An event
/// with no time at all is kept: its age can't be told.
pub(super) fn within_window(
    rows: &[EventRow],
    window: PodEventsWindow,
    now: Timestamp,
) -> (Vec<EventRow>, usize) {
    let Some(span) = window.span() else {
        return (rows.to_vec(), 0);
    };
    let shown: Vec<EventRow> = rows
        .iter()
        .filter(|row| {
            row.last_seen
                .is_none_or(|seen| now.duration_since(seen).unsigned_abs() <= span)
        })
        .cloned()
        .collect();
    let hidden = rows.len() - shown.len();
    (shown, hidden)
}

impl PodDetailPanel {
    /// Starts watching `pod`'s events with `client`, unless this panel already
    /// watches that same pod.
    pub(super) fn watch_events(&mut self, pod: &Pod, client: kube::Client, cx: &mut Context<Self>) {
        let uid = pod.metadata.uid.clone().unwrap_or_default();
        if self.events.as_ref().is_some_and(|watch| watch.uid == uid) {
            return;
        }
        let table = cx.new(|_| EventsTable::default());
        cx.observe(&table, |_, _, cx| cx.notify()).detach();
        let context_name = self.scope.context_name.clone();
        let object = InvolvedObject {
            kind: "Pod",
            namespace: Some(&self.pod.namespace),
            name: &self.pod.name,
            uid: non_empty(&pod.metadata.uid),
        };
        let task = events::watch(
            client,
            &object,
            table.clone(),
            move |cx| ClusterRegistry::handle_unauthorized(cx, &context_name),
            cx,
        );
        let tick = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(crate::consts::POD_EVENTS_WINDOW_TICK)
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    return;
                }
            }
        });
        self.events = Some(PodEventsWatch {
            uid,
            table,
            _task: task,
            _tick: tick,
        });
    }

    /// The Events tab's content within the panel's window at `now`. `None` until
    /// the pod has loaded and its watch has started.
    pub(super) fn events_view(&self, now: Timestamp, cx: &App) -> Option<EventsView> {
        let table = self.events.as_ref()?.table.read(cx);
        Some(match table.refused() {
            Some(refused) => EventsView {
                events: Err(refused.to_string()),
                hidden: 0,
            },
            None => {
                let (shown, hidden) = within_window(table.rows(), self.events_window, now);
                EventsView {
                    events: Ok(events::summarize_rows(&shown, now)),
                    hidden,
                }
            }
        })
    }
}
