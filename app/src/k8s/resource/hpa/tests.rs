//! #156: an HPA's numbers and metrics, as the list and detail panels show them.

use super::{HpaSummary, MetricSummary, ScaleTarget, summarize};
use kube::api::DynamicObject;
use serde_json::json;

fn object(value: serde_json::Value) -> DynamicObject {
    serde_json::from_value(value).expect("an object")
}

fn v2_hpa() -> DynamicObject {
    object(json!({
        "apiVersion": "autoscaling/v2", "kind": "HorizontalPodAutoscaler",
        "metadata": { "name": "web", "namespace": "shop" },
        "spec": {
            "scaleTargetRef": { "apiVersion": "apps/v1", "kind": "Deployment", "name": "web" },
            "minReplicas": 2, "maxReplicas": 10,
            "metrics": [
                { "type": "Resource", "resource": { "name": "cpu",
                  "target": { "type": "Utilization", "averageUtilization": 80 } } },
                { "type": "Resource", "resource": { "name": "memory",
                  "target": { "type": "AverageValue", "averageValue": "1Gi" } } },
                { "type": "Pods", "pods": { "metric": { "name": "requests-per-second" },
                  "target": { "type": "AverageValue", "averageValue": "100" } } },
                { "type": "External", "external": { "metric": { "name": "queue-depth" },
                  "target": { "type": "Value", "value": "30" } } },
            ],
        },
        "status": {
            "currentReplicas": 3, "desiredReplicas": 4,
            // Memory before CPU, and no external reading yet.
            "currentMetrics": [
                { "type": "Resource", "resource": { "name": "memory",
                  "current": { "averageValue": "300Mi" } } },
                { "type": "Resource", "resource": { "name": "cpu",
                  "current": { "averageUtilization": 45, "averageValue": "120m" } } },
                { "type": "Pods", "pods": { "metric": { "name": "requests-per-second" },
                  "current": { "averageValue": "85" } } },
            ],
            "conditions": [
                { "type": "AbleToScale", "status": "True" },
                { "type": "ScalingLimited", "status": "False" },
            ],
        },
    }))
}

#[test]
fn a_v2_hpa_reads_its_bounds_replicas_and_every_metric() {
    let hpa = summarize(&v2_hpa()).expect("an HPA");
    assert_eq!(
        hpa.target,
        ScaleTarget {
            group: "apps".into(),
            kind: "Deployment".into(),
            name: "web".into(),
        }
    );
    assert_eq!((hpa.min_replicas, hpa.max_replicas), (2, 10));
    assert_eq!(
        (hpa.current_replicas, hpa.desired_replicas),
        (Some(3), Some(4))
    );
    assert_eq!(
        hpa.targets(),
        "cpu: 45%/80%, memory: 300Mi/1Gi, requests-per-second: 85/100, \
         queue-depth: <unknown>/30"
    );
    assert_eq!(
        hpa.conditions,
        [
            ("AbleToScale".to_string(), "True".to_string()),
            ("ScalingLimited".to_string(), "False".to_string()),
        ]
    );
}

#[test]
fn a_v1_hpa_reads_its_cpu_target() {
    let hpa = summarize(&object(json!({
        "apiVersion": "autoscaling/v1", "kind": "HorizontalPodAutoscaler",
        "metadata": { "name": "web", "namespace": "shop" },
        "spec": {
            "scaleTargetRef": { "apiVersion": "apps/v1", "kind": "Deployment", "name": "web" },
            "maxReplicas": 5, "targetCPUUtilizationPercentage": 70,
        },
        "status": { "currentReplicas": 1, "desiredReplicas": 2, "currentCPUUtilizationPercentage": 95 },
    })))
    .expect("an HPA");
    assert_eq!(
        hpa,
        HpaSummary {
            target: ScaleTarget {
                group: "apps".into(),
                kind: "Deployment".into(),
                name: "web".into(),
            },
            min_replicas: 1,
            max_replicas: 5,
            current_replicas: Some(1),
            desired_replicas: Some(2),
            metrics: vec![MetricSummary {
                name: "cpu".into(),
                current: Some("95%".into()),
                target: "70%".into(),
            }],
            conditions: Vec::new(),
        }
    );
}

#[test]
fn a_new_hpa_without_status_has_no_readings() {
    let mut fresh = v2_hpa();
    fresh.data.as_object_mut().unwrap().remove("status");
    let hpa = summarize(&fresh).expect("an HPA");
    assert_eq!((hpa.current_replicas, hpa.desired_replicas), (None, None));
    assert!(hpa.metrics.iter().all(|metric| metric.current.is_none()));
}
