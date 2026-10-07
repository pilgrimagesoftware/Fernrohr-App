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
