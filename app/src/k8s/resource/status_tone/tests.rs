use super::*;
use k8s_openapi::api::batch::v1::{JobCondition, JobSpec, JobStatus as Status};

#[test]
fn readiness_grades_ready_replicas_against_desired() {
    assert_eq!(readiness(3, 3), Tone::Good);
    assert_eq!(readiness(4, 3), Tone::Good);
    assert_eq!(readiness(2, 4), Tone::Warning);
    assert_eq!(readiness(1, 4), Tone::Serious);
    assert_eq!(readiness(0, 3), Tone::Bad);
    assert_eq!(readiness(0, 0), Tone::Neutral);
}

fn job(conditions: &[(&str, &str)], suspend: Option<bool>) -> Job {
    Job {
        spec: Some(JobSpec {
            suspend,
            ..Default::default()
        }),
        status: Some(Status {
            conditions: Some(
                conditions
                    .iter()
                    .map(|(type_, status)| JobCondition {
                        type_: (*type_).into(),
                        status: (*status).into(),
                        ..Default::default()
                    })
                    .collect(),
            ),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[test]
fn job_status_follows_conditions_then_suspension() {
    let cases = [
        (
            job(&[("Complete", "True")], None),
            JobStatus::Complete,
            Tone::Good,
        ),
        (
            job(&[("Failed", "True")], None),
            JobStatus::Failed,
            Tone::Bad,
        ),
        (
            job(&[("Failed", "False")], Some(true)),
            JobStatus::Suspended,
            Tone::Neutral,
        ),
        (job(&[], None), JobStatus::Running, Tone::Info),
    ];
    for (job, status, tone) in cases {
        assert_eq!(JobStatus::of(&job), status);
        assert_eq!(status.tone(), tone);
    }
    assert_eq!(JobStatus::Complete.to_string(), "Complete");
}

#[test]
fn phases_take_their_severity() {
    assert_eq!(claim_phase("Bound"), Tone::Good);
    assert_eq!(claim_phase("Pending"), Tone::Info);
    assert_eq!(claim_phase("Lost"), Tone::Bad);
    assert_eq!(volume_phase("Available"), Tone::Good);
    assert_eq!(volume_phase("Released"), Tone::Warning);
    assert_eq!(volume_phase("Failed"), Tone::Bad);
    assert_eq!(namespace_phase("Active"), Tone::Good);
    assert_eq!(namespace_phase("Terminating"), Tone::Warning);
    assert_eq!(namespace_phase("Something"), Tone::Neutral);
}

#[test]
fn node_tone_reads_ready_and_cordon() {
    assert_eq!(node(Some("True"), false), Tone::Good);
    assert_eq!(node(Some("True"), true), Tone::Warning);
    assert_eq!(node(Some("False"), false), Tone::Bad);
    assert_eq!(node(Some("Unknown"), false), Tone::Bad);
    assert_eq!(node(None, false), Tone::Neutral);
}

fn parsed(value: serde_json::Value) -> Vec<serde_json::Value> {
    value.as_array().cloned().unwrap_or_default()
}

#[test]
fn a_ready_condition_gives_its_message_toned_by_status() {
    for (status, tone) in [
        ("True", Tone::Good),
        ("False", Tone::Bad),
        ("Unknown", Tone::Warning),
        ("Sideways", Tone::Warning),
    ] {
        let conditions = parsed(serde_json::json!([
            { "type": "Reconciling", "status": "True", "message": "newer",
              "lastTransitionTime": "2026-10-02T00:00:00Z" },
            { "type": "Ready", "status": status, "message": "install succeeded",
              "lastTransitionTime": "2026-10-01T00:00:00Z" },
        ]));
        assert_eq!(
            condition_message(&conditions),
            Some(("install succeeded".to_string(), tone)),
            "Ready {status}"
        );
    }
}

#[test]
fn a_ready_condition_with_no_message_gives_none_not_another() {
    let conditions = parsed(serde_json::json!([
        { "type": "Ready", "status": "True", "message": "" },
        { "type": "Reconciling", "status": "True", "message": "working" },
    ]));
    assert_eq!(condition_message(&conditions), None);
    assert_eq!(ready_status(&conditions), Some(ConditionStatus::True));
}

#[test]
fn without_ready_the_latest_transition_with_a_message_is_untoned() {
    let conditions = parsed(serde_json::json!([
        { "type": "Available", "status": "True", "message": "older",
          "lastTransitionTime": "2026-10-01T00:00:00Z" },
        { "type": "Progressing", "status": "True", "message": "rolled out",
          "lastTransitionTime": "2026-10-03T00:00:00Z" },
        { "type": "Stalled", "status": "False", "message": "",
          "lastTransitionTime": "2026-10-04T00:00:00Z" },
    ]));
    assert_eq!(
        condition_message(&conditions),
        Some(("rolled out".to_string(), Tone::Neutral))
    );
    assert_eq!(ready_status(&conditions), None);
}

#[test]
fn a_missing_or_malformed_transition_time_counts_as_oldest() {
    let conditions = parsed(serde_json::json!([
        { "type": "A", "status": "True", "message": "dated",
          "lastTransitionTime": "2026-10-01T00:00:00Z" },
        { "type": "B", "status": "True", "message": "garbled",
          "lastTransitionTime": "yesterday-ish" },
        { "type": "C", "status": "True", "message": "undated" },
    ]));
    assert_eq!(
        condition_message(&conditions),
        Some(("dated".to_string(), Tone::Neutral))
    );
    let undated = parsed(serde_json::json!([
        { "type": "A", "status": "True", "message": "first" },
        { "type": "B", "status": "True", "message": "second" },
    ]));
    assert_eq!(
        condition_message(&undated),
        Some(("second".to_string(), Tone::Neutral))
    );
}

#[test]
fn no_conditions_give_no_message() {
    assert_eq!(condition_message(&[]), None);
    let silent = parsed(serde_json::json!([{ "type": "Available", "status": "True" }]));
    assert_eq!(condition_message(&silent), None);
}

#[test]
fn conditions_are_read_from_an_objects_status() {
    let object = serde_json::json!({ "status": { "conditions": [{ "type": "Ready" }] } });
    assert_eq!(conditions_of(&object).len(), 1);
    assert!(conditions_of(&serde_json::json!({ "spec": {} })).is_empty());
    assert!(conditions_of(&serde_json::json!({ "status": { "conditions": "no" } })).is_empty());
}

#[test]
fn condition_statuses_sort_false_unknown_true() {
    let mut statuses = [
        ConditionStatus::True,
        ConditionStatus::Unknown,
        ConditionStatus::False,
    ];
    statuses.sort();
    assert_eq!(
        statuses,
        [
            ConditionStatus::False,
            ConditionStatus::Unknown,
            ConditionStatus::True
        ]
    );
    assert_eq!(ConditionStatus::parse(None), ConditionStatus::Unknown);
    assert_eq!(ConditionStatus::False.to_string(), "False");
}
