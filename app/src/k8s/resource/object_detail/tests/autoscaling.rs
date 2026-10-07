//! #156: a HorizontalPodAutoscaler's detail - what it scales, as a link; its
//! replicas against its bounds; its conditions; each metric against its
//! target.

use super::fixtures::{kind, object};
use super::sections::field;
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::object_detail::model::FieldValue;
use crate::k8s::resource::object_detail::sections::sections_for;
use crate::ui::detail::BadgeTone;
use serde_json::json;

#[test]
fn an_hpa_shows_its_target_replicas_conditions_and_metrics() {
    let hpa = object(json!({
        "apiVersion": "autoscaling/v2", "kind": "HorizontalPodAutoscaler",
        "metadata": { "name": "web", "namespace": "shop" },
        "spec": {
            "scaleTargetRef": { "apiVersion": "apps/v1", "kind": "Deployment", "name": "web" },
            "minReplicas": 2, "maxReplicas": 10,
            "metrics": [
                { "type": "Resource", "resource": { "name": "cpu",
                  "target": { "type": "Utilization", "averageUtilization": 80 } } },
                { "type": "External", "external": { "metric": { "name": "queue-depth" },
                  "target": { "type": "Value", "value": "30" } } },
            ],
        },
        "status": {
            "currentReplicas": 10, "desiredReplicas": 12,
            "currentMetrics": [{ "type": "Resource", "resource": { "name": "cpu",
                "current": { "averageUtilization": 140 } } }],
            "conditions": [
                { "type": "AbleToScale", "status": "True" },
                { "type": "ScalingLimited", "status": "True" },
            ],
        },
    }));

    let sections = sections_for(
        &kind("autoscaling", "v2", "HorizontalPodAutoscaler", true),
        &hpa,
    );

    assert_eq!(
        field(&sections, "Scale target").value,
        FieldValue::References {
            targets: vec![ObjectRef::namespaced("apps", "Deployment", "shop", "web")],
            qualified: true,
        }
    );
    assert_eq!(
        field(&sections, "Replicas").value.text(),
        "current 10 · desired 12 · min 2 · max 10"
    );
    assert_eq!(
        field(&sections, "Conditions").value,
        FieldValue::Badges(vec![
            ("AbleToScale".into(), BadgeTone::Good),
            ("ScalingLimited".into(), BadgeTone::Warning),
        ])
    );
    assert_eq!(field(&sections, "cpu").value.text(), "140% (target 80%)");
    assert_eq!(
        field(&sections, "queue-depth").value.text(),
        "no reading yet (target 30)"
    );
}
