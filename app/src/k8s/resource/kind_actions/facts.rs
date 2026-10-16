//! What the kind-specific actions read of an object, taken once when a list
//! row or a detail panel gets it: small and kind-agnostic - each field is read
//! wherever the object has it, and which actions apply is the kind's call
//! ([`super::KindAction::offered`]).

use kube::api::DynamicObject;
use serde_json::Value;
use std::fmt;
use std::str::FromStr;

/// The parts of an object its kind's actions need.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ObjectFacts {
    pub(crate) uid: Option<String>,
    /// For a patch that replaces a whole list (taints): the list it read.
    pub(crate) resource_version: Option<String>,
    /// A Job's or CronJob's `spec.suspend`.
    pub(crate) suspended: bool,
    /// A Deployment's `spec.paused`.
    pub(crate) paused: bool,
    /// A PersistentVolumeClaim's `spec.resources.requests.storage`.
    pub(crate) storage: Option<String>,
    /// A Node's `spec.taints`, as the API has them.
    pub(crate) taints: Vec<Taint>,
    /// An Ingress's first rule's host that can be opened ([`openable`]): a
    /// wildcard host (`*.example.com`) names no one site.
    pub(crate) host: Option<String>,
    /// The pods a workload or Service selects: a workload's `spec.selector`
    /// label selector, or a Service's `spec.selector` label map. `None` for an
    /// object that selects none.
    pub(crate) pod_selector: Option<kube::core::Selector>,
    /// The kind and name of the object's controlling owner - a ReplicaSet's
    /// Deployment - from its `ownerReferences`.
    pub(crate) controller: Option<(String, String)>,
}

impl ObjectFacts {
    pub(crate) fn of(object: &DynamicObject) -> Self {
        let spec = &object.data["spec"];
        Self {
            uid: object.metadata.uid.clone(),
            resource_version: object.metadata.resource_version.clone(),
            suspended: spec["suspend"].as_bool().unwrap_or(false),
            paused: spec["paused"].as_bool().unwrap_or(false),
            storage: spec["resources"]["requests"]["storage"]
                .as_str()
                .map(str::to_string),
            taints: spec["taints"]
                .as_array()
                .map(|taints| taints.iter().filter_map(Taint::from_api).collect())
                .unwrap_or_default(),
            host: spec["rules"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|rule| rule["host"].as_str())
                .find(|host| openable(host))
                .map(str::to_string),
            pod_selector: pod_selector(&spec["selector"]),
            controller: object
                .metadata
                .owner_references
                .iter()
                .flatten()
                .find(|owner| owner.controller == Some(true))
                .map(|owner| (owner.kind.clone(), owner.name.clone())),
        }
    }
}

/// `spec.selector` as a selector: a `LabelSelector` (`matchLabels`,
/// `matchExpressions`) as workloads have, or a plain label map as a Service
/// has. `None` when there is none, or it selects everything.
fn pod_selector(selector: &Value) -> Option<kube::core::Selector> {
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
    use kube::core::{Expression, Selector};
    let object = selector.as_object()?;
    let selector: Selector = if object.contains_key("matchLabels")
        || object.contains_key("matchExpressions")
    {
        let label_selector: LabelSelector = serde_json::from_value(selector.clone()).ok()?;
        label_selector.try_into().ok()?
    } else {
        object
            .iter()
            .filter_map(|(key, value)| Some(Expression::Equal(key.clone(), value.as_str()?.into())))
            .collect()
    };
    (!selector.selects_all()).then_some(selector)
}

/// Whether `host` is a DNS name a browser can open: lowercase letters, digits,
/// `-` and `.`, in labels of 1 to 63 characters that don't start or end with
/// `-`, 253 at most in all. A wildcard (`*.example.com`) isn't one.
pub(crate) fn openable(host: &str) -> bool {
    let label_ok = |label: &str| {
        (1..=63).contains(&label.len())
            && label
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    };
    !host.is_empty() && host.len() <= 253 && host.split('.').all(label_ok)
}

/// A taint's effect: what it does to pods that don't tolerate it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TaintEffect {
    NoSchedule,
    PreferNoSchedule,
    NoExecute,
}

impl TaintEffect {
    pub(crate) const ALL: [Self; 3] = [Self::NoSchedule, Self::PreferNoSchedule, Self::NoExecute];
}

impl fmt::Display for TaintEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoSchedule => "NoSchedule",
            Self::PreferNoSchedule => "PreferNoSchedule",
            Self::NoExecute => "NoExecute",
        })
    }
}

impl FromStr for TaintEffect {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|effect| effect.to_string().eq_ignore_ascii_case(text))
            .ok_or_else(|| "The effect is NoSchedule, PreferNoSchedule or NoExecute.".to_string())
    }
}

/// One taint. A taint read from the cluster keeps every field as the API sent
/// it (`timeAdded` too), so removing another writes it back unchanged.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Taint {
    pub(crate) key: String,
    pub(crate) value: Option<String>,
    /// As the API spells it: one of [`TaintEffect`]'s, from any real cluster.
    pub(crate) effect: String,
    pub(crate) raw: Value,
}

impl Taint {
    fn from_api(raw: &Value) -> Option<Self> {
        Some(Self {
            key: raw["key"].as_str()?.to_string(),
            value: raw["value"]
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            effect: raw["effect"].as_str().unwrap_or_default().to_string(),
            raw: raw.clone(),
        })
    }

    /// A new taint, as Add Taint writes it.
    pub(crate) fn new(key: &str, value: Option<&str>, effect: TaintEffect) -> Self {
        let mut raw = serde_json::json!({ "key": key, "effect": effect.to_string() });
        if let Some(value) = value {
            raw["value"] = Value::String(value.to_string());
        }
        Self {
            key: key.to_string(),
            value: value.map(str::to_string),
            effect: effect.to_string(),
            raw,
        }
    }

    /// Whether this is `other`'s taint: the same key and effect, as `kubectl
    /// taint` tells taints apart.
    pub(crate) fn same_as(&self, other: &Taint) -> bool {
        self.key == other.key && self.effect == other.effect
    }

    /// `key=value:Effect`, or `key:Effect` with no value - `kubectl taint`'s
    /// spelling.
    pub(crate) fn label(&self) -> String {
        match &self.value {
            Some(value) => format!("{}={value}:{}", self.key, self.effect),
            None => format!("{}:{}", self.key, self.effect),
        }
    }
}
