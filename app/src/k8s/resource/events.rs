//! The events naming one object, for any detail view: how to select them,
//! list them, and summarize each for display.
//!
//! Moved out of pod detail when the generic object viewer needed the same
//! thing (`resource-links` 5.1). The shape of an event, and of the selector
//! that finds an object's events, does not depend on the object's kind.

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

/// When an event last happened. The legacy `lastTimestamp`/`firstTimestamp`
/// pair is empty on events written through `events.k8s.io/v1` (the scheduler's
/// `Scheduled`, among others), which record `series.lastObservedTime` and
/// `eventTime` instead - reading only the legacy pair would sort those last and
/// age them "unknown".
fn event_time(event: &K8sEvent) -> Option<Timestamp> {
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

/// Each event's age-and-tone summary, newest first. `Warning`-type events read
/// as a warning tone and any other type (chiefly `Normal`) as good; an event
/// with no type at all is `Unknown`, neither good nor a warning.
pub fn summarize(events: &[K8sEvent], now: Timestamp) -> Vec<EventSummary> {
    let mut events: Vec<(&K8sEvent, Option<Timestamp>)> = events
        .iter()
        .map(|event| (event, event_time(event)))
        .collect();
    // `Option`'s ordering puts `None` first, so reversing it sorts newest
    // first and leaves undated events at the end.
    events.sort_by(|(_, a), (_, b)| b.cmp(a));
    events
        .into_iter()
        .map(|(event, time)| {
            let age = time
                .map(|time| format_age(now.duration_since(time).as_secs()))
                .unwrap_or_else(|| "unknown".to_string());
            let tone = match event.type_.as_deref() {
                Some("Warning") => BadgeTone::Warning,
                Some(_) => BadgeTone::Good,
                None => BadgeTone::Unknown,
            };
            // Like the timestamps, a series-style event counts its repeats on
            // `series.count` rather than the legacy `count`.
            let count = event
                .count
                .or_else(|| event.series.as_ref().and_then(|series| series.count))
                .unwrap_or(1);
            let text = |value: &Option<String>| {
                value
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            };
            EventSummary {
                reason: text(&event.reason).unwrap_or_else(|| "Unknown".to_string()),
                message: text(&event.message).unwrap_or_default(),
                count,
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
