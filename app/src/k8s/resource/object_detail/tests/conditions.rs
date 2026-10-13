//! `resource-status-message` in the object panel: the Message field beside
//! the Conditions badges of kinds that already show them (3.1, 3.2), and the
//! generic Status section for a kind with no section of its own (4.1-4.3).

use super::fixtures::{deployments, kind, object};
use super::sections::field;
use crate::k8s::resource::object_detail::model::{FieldValue, ObjectSection};
use crate::k8s::resource::object_detail::sections::sections_for;
use crate::k8s::resource::status_tone::ConditionStatus;
use crate::ui::detail::BadgeTone;
use crate::ui::style::Tone;
use serde_json::json;

fn labels(sections: &[ObjectSection]) -> Vec<&str> {
    sections
        .iter()
        .flat_map(|section| &section.fields)
        .map(|field| field.label.as_str())
        .collect()
}

/// 3.1, and the spec's "A Deployment shows its message alongside its
/// condition badges": no `Ready`, so the latest transition's message,
/// untoned, right before the unchanged badges.
#[test]
fn a_deployment_shows_its_latest_message_before_its_conditions() {
    let deployment = object(json!({
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": { "name": "web", "namespace": "staging" },
        "spec": { "replicas": 1, "selector": {}, "template": {} },
        "status": {
            "readyReplicas": 1,
            "conditions": [
                { "type": "Available", "status": "True",
                  "message": "Deployment has minimum availability.",
                  "lastTransitionTime": "2026-10-01T00:00:00Z" },
                { "type": "Progressing", "status": "True",
                  "message": "ReplicaSet \"web-5d4f\" has successfully progressed.",
                  "lastTransitionTime": "2026-10-02T00:00:00Z" },
            ],
        },
    }));

    let sections = sections_for(&deployments(), &deployment);

    assert_eq!(
        field(&sections, "Message").value,
        FieldValue::Message {
            text: "ReplicaSet \"web-5d4f\" has successfully progressed.".into(),
            tone: Tone::Neutral,
        }
    );
    let labels = labels(&sections);
    let message = labels.iter().position(|label| *label == "Message");
    assert_eq!(
        labels.get(message.unwrap() + 1),
        Some(&"Conditions"),
        "Message sits right before Conditions: {labels:?}"
    );
    assert_eq!(
        field(&sections, "Conditions").value,
        FieldValue::Badges(vec![
            ("Available".into(), BadgeTone::Good),
            ("Progressing".into(), BadgeTone::Good),
        ])
    );
}

/// 3.2: conditions with no message keep their badges and gain no Message.
#[test]
fn conditions_without_a_message_show_no_message_field() {
    let deployment = object(json!({
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": { "name": "web", "namespace": "staging" },
        "spec": { "selector": {}, "template": {} },
        "status": { "conditions": [{ "type": "Available", "status": "True" }] },
    }));

    let sections = sections_for(&deployments(), &deployment);

    let labels = labels(&sections);
    assert!(!labels.contains(&"Message"), "no Message field: {labels:?}");
    assert_eq!(field(&sections, "Conditions").value.text(), "Available");
}

/// 4.1, 4.3, and the spec's failing-HelmRelease scenarios: a kind with no
/// section of its own gets a Status section - Ready, its message in the Bad
/// tone, and every condition.
#[test]
fn a_failing_helm_release_gets_a_status_section() {
    let release = object(json!({
        "apiVersion": "helm.toolkit.fluxcd.io/v2",
        "kind": "HelmRelease",
        "metadata": { "name": "web", "namespace": "flux-system" },
        "status": {
            "conditions": [
                { "type": "Ready", "status": "False",
                  "message": "Helm upgrade failed: timed out waiting for the condition",
                  "lastTransitionTime": "2026-10-01T00:00:00Z" },
                { "type": "Released", "status": "False",
                  "message": "Helm upgrade failed",
                  "lastTransitionTime": "2026-10-02T00:00:00Z" },
            ],
        },
    }));

    let sections = sections_for(
        &kind("helm.toolkit.fluxcd.io", "v2", "HelmRelease", true),
        &release,
    );

    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].title, "Status");
    assert_eq!(labels(&sections), ["Ready", "Message", "Conditions"]);
    assert_eq!(
        field(&sections, "Ready").value,
        FieldValue::Ready(ConditionStatus::False)
    );
    assert_eq!(
        field(&sections, "Message").value,
        FieldValue::Message {
            text: "Helm upgrade failed: timed out waiting for the condition".into(),
            tone: Tone::Bad,
        }
    );
    assert_eq!(
        field(&sections, "Conditions").value,
        FieldValue::Badges(vec![
            ("Ready".into(), BadgeTone::Warning),
            ("Released".into(), BadgeTone::Warning),
        ])
    );
}

/// 4.1: a Kustomization's Status section, Ready and healthy.
#[test]
fn a_healthy_kustomization_gets_a_good_status_section() {
    let kustomization = object(json!({
        "apiVersion": "kustomize.toolkit.fluxcd.io/v1",
        "kind": "Kustomization",
        "metadata": { "name": "apps", "namespace": "flux-system" },
        "status": {
            "conditions": [{ "type": "Ready", "status": "True",
                             "message": "Applied revision: main@sha1:0123" }],
        },
    }));

    let sections = sections_for(
        &kind("kustomize.toolkit.fluxcd.io", "v1", "Kustomization", true),
        &kustomization,
    );

    assert_eq!(
        field(&sections, "Message").value,
        FieldValue::Message {
            text: "Applied revision: main@sha1:0123".into(),
            tone: Tone::Good,
        }
    );
    assert_eq!(field(&sections, "Ready").value.text(), "True");
}

/// 4.2: a CRD object with no conditions keeps Overview only.
#[test]
fn a_crd_without_conditions_has_no_sections() {
    let widget = object(json!({
        "apiVersion": "example.com/v1",
        "kind": "Widget",
        "metadata": { "name": "gear", "namespace": "staging" },
        "spec": { "size": 3 },
        "status": { "phase": "Ready" },
    }));

    assert!(sections_for(&kind("example.com", "v1", "Widget", true), &widget).is_empty());
}
