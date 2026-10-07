//! A HorizontalPodAutoscaler read for display (#156): what it scales, within
//! which bounds, how many replicas it has and wants, and each metric's current
//! value beside its target - the numbers `kubectl get hpa` and `describe`
//! show. One [`HpaSummary`] feeds both the list's columns and the detail
//! panel's sections, from either `autoscaling/v2` or the older `v1` shape.

use k8s_openapi::api::autoscaling::{v1, v2};
use kube::api::DynamicObject;

/// What an HPA scales: the target's API group, kind and name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScaleTarget {
    pub group: String,
    pub kind: String,
    pub name: String,
}

/// One metric the HPA scales on: what it measures, its target, and its
/// current value when the HPA has observed one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MetricSummary {
    /// `cpu`, `memory (container app)`, `requests-per-second`,
    /// `queue-depth on Service/web` ...
    pub name: String,
    pub current: Option<String>,
    pub target: String,
}

impl MetricSummary {
    /// `cpu: 45%/80%`, or `cpu: <unknown>/80%` before the HPA has a reading,
    /// as `kubectl get hpa`'s Targets column reads.
    pub(crate) fn short(&self) -> String {
        let current = self.current.as_deref().unwrap_or("<unknown>");
        format!("{}: {current}/{}", self.name, self.target)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HpaSummary {
    pub target: ScaleTarget,
    /// The API's default when unset is 1.
    pub min_replicas: i32,
    pub max_replicas: i32,
    pub current_replicas: Option<i32>,
    pub desired_replicas: Option<i32>,
    pub metrics: Vec<MetricSummary>,
    /// `(type, status)` pairs: AbleToScale, ScalingActive, ScalingLimited.
    pub conditions: Vec<(String, String)>,
}

impl HpaSummary {
    /// The Targets column: every metric's [`MetricSummary::short`], joined.
    pub(crate) fn targets(&self) -> String {
        self.metrics
            .iter()
            .map(MetricSummary::short)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// `object` read as an HPA, by its `apiVersion` - `None` when it doesn't
/// deserialize as one.
pub(crate) fn summarize(object: &DynamicObject) -> Option<HpaSummary> {
    let value = serde_json::to_value(object).ok()?;
    match value.get("apiVersion").and_then(|version| version.as_str()) {
        Some("autoscaling/v1") => serde_json::from_value(value).ok().map(|hpa| from_v1(&hpa)),
        _ => serde_json::from_value(value).ok().map(|hpa| from_v2(&hpa)),
    }
}

fn group_of(api_version: Option<&str>) -> String {
    api_version
        .and_then(|version| version.split_once('/'))
        .map(|(group, _)| group.to_string())
        .unwrap_or_default()
}

pub(crate) fn from_v2(hpa: &v2::HorizontalPodAutoscaler) -> HpaSummary {
    let spec = &hpa.spec;
    let status = hpa.status.as_ref();
    // Each observed metric as `(name, value)`, read once.
    let observed: Vec<(String, Option<String>)> = status
        .and_then(|status| status.current_metrics.as_deref())
        .unwrap_or_default()
        .iter()
        .filter_map(status_metric)
        .collect();
    let metrics = spec
        .metrics
        .as_deref()
        .unwrap_or_default()
        .iter()
        .enumerate()
        .filter_map(|(index, metric)| {
            let (name, target) = spec_metric(metric)?;
            // The status lists metrics in the spec's order; where it doesn't,
            // match on what's measured.
            let observed = observed
                .get(index)
                .filter(|(seen, _)| *seen == name)
                .or_else(|| observed.iter().find(|(seen, _)| *seen == name))
                .and_then(|(_, value)| value.clone());
            Some(MetricSummary {
                name,
                current: observed,
                target,
            })
        })
        .collect();
    HpaSummary {
        target: ScaleTarget {
            group: group_of(spec.scale_target_ref.api_version.as_deref()),
            kind: spec.scale_target_ref.kind.clone(),
            name: spec.scale_target_ref.name.clone(),
        },
        min_replicas: spec.min_replicas.unwrap_or(1),
        max_replicas: spec.max_replicas,
        current_replicas: status.and_then(|status| status.current_replicas),
        desired_replicas: status.map(|status| status.desired_replicas),
        metrics,
        conditions: status
            .and_then(|status| status.conditions.as_deref())
            .unwrap_or_default()
            .iter()
            .map(|condition| (condition.type_.clone(), condition.status.clone()))
            .collect(),
    }
}

pub(crate) fn from_v1(hpa: &v1::HorizontalPodAutoscaler) -> HpaSummary {
    let spec = &hpa.spec;
    let status = hpa.status.as_ref();
    let metrics = spec
        .target_cpu_utilization_percentage
        .map(|target| MetricSummary {
            name: "cpu".into(),
            current: status
                .and_then(|status| status.current_cpu_utilization_percentage)
                .map(|current| format!("{current}%")),
            target: format!("{target}%"),
        })
        .into_iter()
        .collect();
    HpaSummary {
        target: ScaleTarget {
            group: group_of(spec.scale_target_ref.api_version.as_deref()),
            kind: spec.scale_target_ref.kind.clone(),
            name: spec.scale_target_ref.name.clone(),
        },
        min_replicas: spec.min_replicas.unwrap_or(1),
        max_replicas: spec.max_replicas,
        current_replicas: status.map(|status| status.current_replicas),
        desired_replicas: status.map(|status| status.desired_replicas),
        metrics,
        conditions: Vec::new(),
    }
}

/// A spec metric's name and its target, as text; `None` for a metric type
/// this doesn't know.
fn spec_metric(metric: &v2::MetricSpec) -> Option<(String, String)> {
    if let Some(resource) = &metric.resource {
        return Some((resource.name.clone(), target_text(&resource.target)));
    }
    if let Some(resource) = &metric.container_resource {
        return Some((
            format!("{} (container {})", resource.name, resource.container),
            target_text(&resource.target),
        ));
    }
    if let Some(pods) = &metric.pods {
        return Some((pods.metric.name.clone(), target_text(&pods.target)));
    }
    if let Some(object) = &metric.object {
        return Some((
            format!(
                "{} on {}/{}",
                object.metric.name, object.described_object.kind, object.described_object.name
            ),
            target_text(&object.target),
        ));
    }
    let external = metric.external.as_ref()?;
    Some((external.metric.name.clone(), target_text(&external.target)))
}

/// A status metric's name, as [`spec_metric`] names it, and its current value.
fn status_metric(metric: &v2::MetricStatus) -> Option<(String, Option<String>)> {
    if let Some(resource) = &metric.resource {
        return Some((resource.name.clone(), value_text(&resource.current)));
    }
    if let Some(resource) = &metric.container_resource {
        return Some((
            format!("{} (container {})", resource.name, resource.container),
            value_text(&resource.current),
        ));
    }
    if let Some(pods) = &metric.pods {
        return Some((pods.metric.name.clone(), value_text(&pods.current)));
    }
    if let Some(object) = &metric.object {
        return Some((
            format!(
                "{} on {}/{}",
                object.metric.name, object.described_object.kind, object.described_object.name
            ),
            value_text(&object.current),
        ));
    }
    let external = metric.external.as_ref()?;
    Some((external.metric.name.clone(), value_text(&external.current)))
}

/// A target as `kubectl` writes it: a utilization as `80%`, an average or
/// total value as its quantity.
fn target_text(target: &v2::MetricTarget) -> String {
    if let Some(utilization) = target.average_utilization {
        return format!("{utilization}%");
    }
    if let Some(average) = &target.average_value {
        return average.0.clone();
    }
    target
        .value
        .as_ref()
        .map_or_else(|| "<unset>".to_string(), |value| value.0.clone())
}

/// A current value, the same way; `None` before the HPA has one.
fn value_text(current: &v2::MetricValueStatus) -> Option<String> {
    if let Some(utilization) = current.average_utilization {
        return Some(format!("{utilization}%"));
    }
    if let Some(average) = &current.average_value {
        return Some(average.0.clone());
    }
    current.value.as_ref().map(|value| value.0.clone())
}

#[cfg(test)]
mod tests;
