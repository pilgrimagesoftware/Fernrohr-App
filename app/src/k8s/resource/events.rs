//! The events naming one object, for any detail view: how to select them,
//! list them, and summarize each for display.
//!
//! Moved out of pod detail when the generic object viewer needed the same
//! thing (`resource-links` 5.1). The shape of an event, and of the selector
//! that finds an object's events, does not depend on the object's kind.

use crate::k8s::resource::events_browser::EventRow;
use crate::k8s::resource::pods::format_age;
use crate::ui::detail::BadgeTone;
use jiff::Timestamp;
use k8s_openapi::api::core::v1::Event as K8sEvent;
use kube::Api;
use kube::api::ListParams;

/// One event, summarized for display, newest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventSummary {
    pub reason: String,
    pub message: String,
    pub count: i32,
    pub age: String,
    pub tone: BadgeTone,
}

/// The object events are listed for: enough to pin them to it and nothing
/// else of the same name.
pub struct InvolvedObject<'a> {
    pub kind: &'a str,
    /// `None` for a cluster-scoped object.
    pub namespace: Option<&'a str>,
    pub name: &'a str,
    /// Pinned when the object has one, so a predecessor recreated under the
    /// same name (a StatefulSet pod, a re-applied ConfigMap) keeps its own
    /// events out.
    pub uid: Option<&'a str>,
}

/// The field selector for `object`'s events. Name alone is not enough: a
/// Service or ReplicaSet can share a pod's name, so the selector also pins the
/// kind, the namespace when there is one, and the UID when known.
pub fn selector(object: &InvolvedObject<'_>) -> String {
    let mut selector = format!("involvedObject.kind={}", object.kind);
    if let Some(namespace) = object.namespace {
        selector.push_str(&format!(",involvedObject.namespace={namespace}"));
    }
    selector.push_str(&format!(",involvedObject.name={}", object.name));
    if let Some(uid) = object.uid.filter(|uid| !uid.is_empty()) {
        selector.push_str(&format!(",involvedObject.uid={uid}"));
    }
    selector
}

/// Lists `object`'s events - in its namespace, or across all namespaces for a
/// cluster-scoped object, whose events the API records in whichever namespace
/// the reporting component chose (usually `default`).
///
/// A failure is kept as its message rather than failing the caller: a user
/// allowed to read an object but not to list events still gets the object.
pub async fn list(
    client: kube::Client,
    object: &InvolvedObject<'_>,
) -> Result<Vec<K8sEvent>, String> {
    let api: Api<K8sEvent> = match object.namespace {
        Some(namespace) => Api::namespaced(client, namespace),
        None => Api::all(client),
    };
    api.list(&ListParams::default().fields(&selector(object)))
        .await
        .map(|list| list.items)
        .map_err(|error| error.to_string())
}

/// Watches `object`'s events - the field selector [`list`] uses - applying each
/// change to `table`, the events browser's model, so a detail view's events read
/// exactly as the browser's do (`pod-events-time-window` D1). One watch per
/// detail panel: the selector is unique to the object, so there's nothing to
/// share. A 403 is recorded on `table` as its refusal; a 401 calls
/// `on_unauthorized` once. Dropping the task ends the watch.
pub fn watch(
    client: kube::Client,
    object: &InvolvedObject<'_>,
    table: gpui_kit::Entity<crate::k8s::resource::events_browser::EventsTable>,
    on_unauthorized: impl FnOnce(&mut gpui_kit::App) + 'static,
    cx: &mut gpui_kit::App,
) -> gpui_kit::Task<()> {
    use crate::k8s::cluster::watch_stream::{self, OnNoResourceVersion, OnRefused};
    let api: Api<K8sEvent> = match object.namespace {
        Some(namespace) => Api::namespaced(client, namespace),
        None => Api::all(client),
    };
    let config = kube_runtime::watcher::Config::default().fields(&selector(object));
    let refused = table.clone();
    watch_stream::run_with(
        api,
        config,
        move |event, cx| {
            table.update(cx, |table, cx| {
                table.apply(event);
                cx.notify();
            });
        },
        on_unauthorized,
        OnRefused::Report(Box::new(move |message, cx| {
            refused.update(cx, |table, cx| {
                table.set_refused(message);
                cx.notify();
            });
        })),
        // An Event list always carries a `resourceVersion`.
        OnNoResourceVersion::Retry,
        cx,
    )
}

/// When an event last happened. The legacy `lastTimestamp`/`firstTimestamp`
/// pair is empty on events written through `events.k8s.io/v1` (the scheduler's
/// `Scheduled`, among others), which record `series.lastObservedTime` and
/// `eventTime` instead - reading only the legacy pair would sort those last and
/// age them "unknown".
pub(crate) fn event_time(event: &K8sEvent) -> Option<Timestamp> {
    event
        .last_timestamp
        .as_ref()
        .map(|time| time.0)
        .or_else(|| {
            event
                .series
                .as_ref()
                .and_then(|series| series.last_observed_time.as_ref())
                .map(|time| time.0)
        })
        .or_else(|| event.event_time.as_ref().map(|time| time.0))
        .or_else(|| event.first_timestamp.as_ref().map(|time| time.0))
}

/// When an event first happened: the legacy `firstTimestamp`, else the
/// `events.k8s.io/v1` `eventTime` an event written through that API records
/// instead.
pub(crate) fn first_seen(event: &K8sEvent) -> Option<Timestamp> {
    event
        .first_timestamp
        .as_ref()
        .map(|time| time.0)
        .or_else(|| event.event_time.as_ref().map(|time| time.0))
}

/// How many times the event has happened: the legacy `count`, else a
/// series-style event's `series.count`, else once.
pub(crate) fn event_count(event: &K8sEvent) -> i32 {
    event
        .count
        .or_else(|| event.series.as_ref().and_then(|series| series.count))
        .unwrap_or(1)
}

/// `value` when it is set and non-empty.
pub(crate) fn text(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// The component that reported the event: `events.k8s.io/v1`'s
/// `reportingComponent`, else the legacy `source.component`.
pub(crate) fn reporter(event: &K8sEvent) -> Option<String> {
    text(&event.reporting_component).or_else(|| {
        event
            .source
            .as_ref()
            .and_then(|source| text(&source.component))
    })
}

/// Which instance of the component reported it: `reportingInstance`, else the
/// legacy `source.host`.
pub(crate) fn reporting_instance(event: &K8sEvent) -> Option<String> {
    text(&event.reporting_instance)
        .or_else(|| event.source.as_ref().and_then(|source| text(&source.host)))
}

/// Each event's age-and-tone summary, newest first. `Warning`-type events read
/// as a warning tone and any other type (chiefly `Normal`) as good; an event
/// with no type at all is `Unknown`, neither good nor a warning.
pub fn summarize(events: &[K8sEvent], now: Timestamp) -> Vec<EventSummary> {
    let rows: Vec<EventRow> = events.iter().map(EventRow::new).collect();
    summarize_rows(&rows, now)
}

/// [`summarize`] for events already read into the events browser's rows - what a
/// live watch keeps (`pod-events-time-window` D1). Both paths go through here, so
/// an event reads the same wherever it's shown.
pub fn summarize_rows(rows: &[EventRow], now: Timestamp) -> Vec<EventSummary> {
    let mut rows: Vec<&EventRow> = rows.iter().collect();
    // `Option`'s ordering puts `None` first, so reversing it sorts newest
    // first and leaves undated events at the end.
    rows.sort_by_key(|row| std::cmp::Reverse(row.last_seen));
    rows.into_iter()
        .map(|row| {
            let age = row
                .last_seen
                .map(|time| format_age(now.duration_since(time).as_secs()))
                .unwrap_or_else(|| "unknown".to_string());
            let tone = match row.type_.as_deref() {
                Some("Warning") => BadgeTone::Warning,
                Some(_) => BadgeTone::Good,
                None => BadgeTone::Unknown,
            };
            EventSummary {
                reason: if row.reason.is_empty() {
                    "Unknown".to_string()
                } else {
                    row.reason.clone()
                },
                message: row.message.clone(),
                count: row.count,
                age,
                tone,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{InvolvedObject, selector};

    #[test]
    fn a_cluster_scoped_objects_selector_has_no_namespace() {
        assert_eq!(
            selector(&InvolvedObject {
                kind: "Node",
                namespace: None,
                name: "node-a",
                uid: Some("u-1"),
            }),
            "involvedObject.kind=Node,involvedObject.name=node-a,involvedObject.uid=u-1"
        );
    }

    #[test]
    fn a_namespaced_objects_selector_pins_its_namespace() {
        assert_eq!(
            selector(&InvolvedObject {
                kind: "ConfigMap",
                namespace: Some("staging"),
                name: "app-config",
                uid: None,
            }),
            "involvedObject.kind=ConfigMap,involvedObject.namespace=staging,\
             involvedObject.name=app-config"
        );
    }
}

#[cfg(test)]
mod watch_tests;
