//! The Event section, for an event recorded through the legacy fields and for
//! one recorded only through `events.k8s.io/v1`'s.

use super::event_at;
use jiff::Timestamp;
use k8s_openapi::api::core::v1::Event;
use serde_json::json;

fn at(time: &str) -> Timestamp {
    time.parse().unwrap()
}

/// Each field's label and its single-line text, in order.
fn fields(event: &Event) -> Vec<(String, String)> {
    event_at(event, at("2026-10-02T12:00:00Z"))
        .into_iter()
        .flat_map(|section| section.fields)
        .map(|field| (field.label.clone(), field.value.text()))
        .collect()
}

fn value<'a>(fields: &'a [(String, String)], label: &str) -> &'a str {
    fields
        .iter()
        .find(|(name, _)| name == label)
        .map(|(_, value)| value.as_str())
        .unwrap_or_else(|| panic!("no {label} in {fields:?}"))
}

#[test]
fn a_legacy_event_shows_what_happened_and_to_what() {
    let event: Event = serde_json::from_value(json!({
        "metadata": { "name": "web-1.1", "namespace": "payments" },
        "involvedObject": { "kind": "Pod", "apiVersion": "v1", "namespace": "payments", "name": "web-1" },
        "type": "Warning", "reason": "BackOff", "message": "Back-off restarting failed container",
        "count": 7,
        "firstTimestamp": "2026-10-02T11:00:00Z", "lastTimestamp": "2026-10-02T11:55:00Z",
        "source": { "component": "kubelet", "host": "node-a" },
    }))
    .unwrap();
    let fields = fields(&event);
    assert_eq!(value(&fields, "Type"), "Warning");
    assert_eq!(value(&fields, "Reason"), "BackOff");
    assert_eq!(
        value(&fields, "Message"),
        "Back-off restarting failed container"
    );
    assert_eq!(value(&fields, "Count"), "7");
    assert_eq!(
        value(&fields, "First Seen"),
        "2026-10-02 11:00:00 UTC (1h ago)"
    );
    assert_eq!(
        value(&fields, "Last Seen"),
        "2026-10-02 11:55:00 UTC (5m ago)"
    );
    assert_eq!(value(&fields, "Reporting Component"), "kubelet");
    assert_eq!(value(&fields, "Reporting Instance"), "node-a");
    assert_eq!(value(&fields, "Involved Object"), "Pod/web-1");
}

#[test]
fn a_v1_only_event_reads_its_v1_fields() {
    let event: Event = serde_json::from_value(json!({
        "metadata": { "name": "web-2.1", "namespace": "payments" },
        "involvedObject": { "kind": "Pod", "apiVersion": "v1", "namespace": "payments", "name": "web-2" },
        "related": { "kind": "Node", "apiVersion": "v1", "name": "node-b" },
        "type": "Normal", "reason": "Scheduled", "message": "Successfully assigned payments/web-2",
        "action": "Binding",
        "eventTime": "2026-10-02T11:00:00.000000Z",
        "series": { "count": 3, "lastObservedTime": "2026-10-02T11:30:00.000000Z" },
        "reportingComponent": "default-scheduler", "reportingInstance": "scheduler-0",
    }))
    .unwrap();
    let fields = fields(&event);
    assert_eq!(value(&fields, "Type"), "Normal");
    assert_eq!(value(&fields, "Count"), "3", "series.count");
    assert_eq!(
        value(&fields, "First Seen"),
        "2026-10-02 11:00:00 UTC (1h ago)",
        "eventTime"
    );
    assert_eq!(
        value(&fields, "Last Seen"),
        "2026-10-02 11:30:00 UTC (30m ago)",
        "series.lastObservedTime"
    );
    assert_eq!(value(&fields, "Reporting Component"), "default-scheduler");
    assert_eq!(value(&fields, "Reporting Instance"), "scheduler-0");
    assert_eq!(value(&fields, "Action"), "Binding");
    assert_eq!(value(&fields, "Related Object"), "Node/node-b");
}
