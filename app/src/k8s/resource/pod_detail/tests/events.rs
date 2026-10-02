//! Event ordering and tone, managed-field entries, and the events selector.

use super::fixtures::{at, rich_pod};
use crate::k8s::resource::events::{EventSummary, InvolvedObject, selector, summarize};
use crate::k8s::resource::pod_detail::format::managed_field_entry;
use crate::k8s::resource::pod_detail::model::{BadgeTone, ManagedFieldEntry};
use crate::k8s::resource::pods::format_age;
use k8s_openapi::api::core::v1::{Event as K8sEvent, EventSeries};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{
    FieldsV1, ManagedFieldsEntry, MicroTime, Time,
};

#[test]
fn events_sort_newest_first_across_legacy_and_series_timestamps() {
    let legacy = K8sEvent {
        reason: Some("Pulled".into()),
        message: Some("Container image already present".into()),
        type_: Some("Normal".into()),
        count: Some(2),
        first_timestamp: Some(Time(at(50))),
        last_timestamp: Some(Time(at(100))),
        ..Default::default()
    };
    // Written through events.k8s.io/v1: no legacy timestamps or count, only
    // `eventTime` and a series.
    let series = K8sEvent {
        reason: Some("BackOff".into()),
        message: Some("Back-off restarting failed container".into()),
        type_: Some("Warning".into()),
        event_time: Some(MicroTime(at(200))),
        series: Some(EventSeries {
            count: Some(4),
            last_observed_time: Some(MicroTime(at(300))),
        }),
        ..Default::default()
    };
    let undated = K8sEvent {
        reason: None,
        ..Default::default()
    };

    let events = summarize(&[undated, legacy, series], at(400));

    assert_eq!(
        events,
        vec![
            EventSummary {
                reason: "BackOff".into(),
                message: "Back-off restarting failed container".into(),
                count: 4,
                age: format_age(100),
                tone: BadgeTone::Warning,
            },
            EventSummary {
                reason: "Pulled".into(),
                message: "Container image already present".into(),
                count: 2,
                age: format_age(300),
                tone: BadgeTone::Good,
            },
            EventSummary {
                reason: "Unknown".into(),
                message: String::new(),
                count: 1,
                age: "unknown".into(),
                tone: BadgeTone::Unknown,
            },
        ],
        "newest first by series time, then legacy time; undated last"
    );
}

#[test]
fn a_managed_fields_entry_pretty_prints_what_it_owns() {
    let ownership = serde_json::json!({ "f:metadata": { "f:labels": { "f:app": {} } } });
    let entry = managed_field_entry(&ManagedFieldsEntry {
        manager: Some("kubectl-client-side-apply".into()),
        operation: Some("Update".into()),
        fields_v1: Some(FieldsV1(ownership.clone())),
        ..Default::default()
    });

    assert_eq!(
        entry,
        ManagedFieldEntry {
            manager: "kubectl-client-side-apply".into(),
            operation: "Update".into(),
            time: None,
            fields_json: serde_json::to_string_pretty(&ownership).unwrap(),
        }
    );
}

#[test]
fn a_bare_managed_fields_entry_is_kept_and_says_what_is_missing() {
    let entry = managed_field_entry(&ManagedFieldsEntry::default());

    assert_eq!(entry.manager, "unknown manager");
    assert_eq!(entry.operation, "unknown operation");
    assert_eq!(entry.fields_json, "(no field ownership recorded)");
}

/// The selector pod detail lists a pod's events with - what `fetch_pod` builds.
fn events_selector(pod: &k8s_openapi::api::core::v1::Pod, namespace: &str, name: &str) -> String {
    selector(&InvolvedObject {
        kind: "Pod",
        namespace: Some(namespace),
        name,
        uid: pod.metadata.uid.as_deref(),
    })
}

#[test]
fn the_events_selector_pins_kind_and_uid() {
    let mut pod = rich_pod();
    pod.metadata.uid = Some("pod-uid-1".into());
    assert_eq!(
        events_selector(&pod, "staging", "api-7d9f-ftg5t"),
        "involvedObject.kind=Pod,involvedObject.namespace=staging,\
         involvedObject.name=api-7d9f-ftg5t,involvedObject.uid=pod-uid-1"
    );

    pod.metadata.uid = None;
    assert_eq!(
        events_selector(&pod, "staging", "api-7d9f-ftg5t"),
        "involvedObject.kind=Pod,involvedObject.namespace=staging,\
         involvedObject.name=api-7d9f-ftg5t",
        "a pod with no UID yet is still matched by kind and name"
    );
}
