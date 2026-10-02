//! The pod's events, live (`pod-events-time-window` 1.2): once the pod has loaded,
//! the panel runs a field-selected Event watch on it (`events::watch`), kept in the
//! events browser's table so both render events alike. The watch is the panel's:
//! it starts when the pod first loads, restarts only if the pod was recreated
//! under the same name (a new uid), and ends when the panel is dropped.

use super::PodDetailPanel;
use super::format::non_empty;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::events::{self, EventSummary, InvolvedObject};
use crate::k8s::resource::events_browser::EventsTable;
use gpui_kit::*;
use jiff::Timestamp;
use k8s_openapi::api::core::v1::Pod;

/// A running watch on one pod's events: which pod (by uid), its table, and the
/// task that keeps the table current - dropping it ends the watch.
pub(super) struct PodEventsWatch {
    uid: String,
    pub(super) table: Entity<EventsTable>,
    _task: Task<()>,
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
        self.events = Some(PodEventsWatch {
            uid,
            table,
            _task: task,
        });
    }

    /// The Events tab's rows, newest first - or why events can't be listed.
    /// `None` until the pod has loaded and its watch has started.
    pub(super) fn event_summaries(
        &self,
        now: Timestamp,
        cx: &App,
    ) -> Option<Result<Vec<EventSummary>, String>> {
        let table = self.events.as_ref()?.table.read(cx);
        Some(match table.refused() {
            Some(refused) => Err(refused.to_string()),
            None => Ok(events::summarize_rows(table.rows(), now)),
        })
    }
}
