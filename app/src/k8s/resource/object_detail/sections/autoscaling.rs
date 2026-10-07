//! A HorizontalPodAutoscaler's sections (#156): what it scales - a link -
//! its replicas against its bounds, how its conditions stand, and each
//! metric's current value against its target. Read through
//! `resource::hpa`, as the list's columns are, so the two never disagree.

use super::super::model::{ObjectField, ObjectSection};
use super::common::condition_badges;
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::hpa::{self, MetricSummary};
use kube::api::DynamicObject;

/// The condition that is bad news when it holds: the HPA wants more (or
/// fewer) replicas than its bounds allow.
fn negative(condition: &str) -> bool {
    condition == "ScalingLimited"
}

/// `45% (target 80%)`, or that there's no reading yet.
fn reading(metric: &MetricSummary) -> String {
    match &metric.current {
        Some(current) => format!("{current} (target {})", metric.target),
        None => format!("no reading yet (target {})", metric.target),
    }
}

/// `object`'s sections, or `None` when it doesn't read as an HPA.
pub(super) fn hpa(object: &DynamicObject, namespace: &str) -> Option<Vec<ObjectSection>> {
    let hpa = hpa::summarize(object)?;
    let replicas = [
        hpa.current_replicas.map(|count| format!("current {count}")),
        hpa.desired_replicas.map(|count| format!("desired {count}")),
        Some(format!("min {}", hpa.min_replicas)),
        Some(format!("max {}", hpa.max_replicas)),
    ];
    let mut scaling = vec![
        ObjectField::references(
            "Scale target",
            vec![ObjectRef::namespaced(
                hpa.target.group.clone(),
                hpa.target.kind.clone(),
                namespace,
                hpa.target.name.clone(),
            )],
            true,
        ),
        ObjectField::text(
            "Replicas",
            replicas
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · "),
        ),
    ];
    let conditions = hpa
        .conditions
        .iter()
        .map(|(condition, status)| (condition.as_str(), status.as_str()));
    if let Some(badges) = condition_badges(conditions, negative) {
        scaling.push(ObjectField::new("Conditions", badges));
    }
    let mut sections = vec![ObjectSection::new("Scaling", scaling)];
    if !hpa.metrics.is_empty() {
        sections.push(ObjectSection::new(
            "Metrics",
            hpa.metrics
                .iter()
                .map(|metric| ObjectField::text(metric.name.clone(), reading(metric)))
                .collect(),
        ));
    }
    Some(sections)
}
