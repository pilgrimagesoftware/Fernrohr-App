//! The action tools' typed inputs, and what each parses into before anything
//! else happens (`agent-mcp`: Typed action requests).
//!
//! Each tool's allowlist is its input type: a workload tool's `kind` is a
//! closed enum of the kinds design.md's table names, and the ConfigMap, Pod
//! and CronJob tools take no kind at all. A DaemonSet passed to
//! `scale_workload`, or a Secret to anything, doesn't parse - it fails as
//! invalid arguments, before a session is looked up, the user is asked, or a
//! request is built. Names go through `names`, so every one is a single safe
//! path segment.

use crate::consts::MCP_DELETE_PODS_MAX;
use crate::mcp::error::ToolError;
use crate::mcp::kinds::KindQuery;
use crate::mcp::names::{LabelName, ObjectName};
use schemars::JsonSchema;
use serde::Deserialize;

/// The kinds `scale_workload` scales.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
pub(super) enum ScalableKind {
    Deployment,
    StatefulSet,
    ReplicaSet,
}

/// The kinds whose rollout `restart_workload` restarts and
/// `rollback_workload` undoes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
pub(super) enum RolloutKind {
    Deployment,
    StatefulSet,
    DaemonSet,
}

impl ScalableKind {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Deployment => "Deployment",
            Self::StatefulSet => "StatefulSet",
            Self::ReplicaSet => "ReplicaSet",
        }
    }
}

impl RolloutKind {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Deployment => "Deployment",
            Self::StatefulSet => "StatefulSet",
            Self::DaemonSet => "DaemonSet",
        }
    }
}

/// The discovery query for an `apps` kind named `kind`.
pub(super) fn apps(kind: &str) -> KindQuery {
    query(kind, "apps")
}

/// The discovery query for `kind` in `group` (`""`: core).
pub(super) fn query(kind: &str, group: &str) -> KindQuery {
    KindQuery {
        kind: kind.to_string(),
        group: Some(if group.is_empty() { "core" } else { group }.to_string()),
    }
}

/// One object an action targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Target {
    pub(super) context: String,
    pub(super) namespace: LabelName,
    pub(super) name: ObjectName,
}

impl Target {
    pub(super) fn parse(context: &str, namespace: &str, name: &str) -> Result<Self, ToolError> {
        Ok(Self {
            context: context.to_string(),
            namespace: LabelName::parse("namespace", namespace)?,
            name: ObjectName::parse("name", name)?,
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct SetConfigMapValueInput {
    /// The context's name, as `list_contexts` reports it.
    pub(super) context: String,
    /// The ConfigMap's namespace.
    pub(super) namespace: String,
    /// The ConfigMap's name.
    pub(super) name: String,
    /// The key in the ConfigMap's `data` to set or remove.
    pub(super) key: String,
    /// The key's new value; omit it, or pass null, to remove the key.
    #[serde(default)]
    pub(super) value: Option<String>,
}

/// The longest ConfigMap key the API server accepts.
const MAX_CONFIGMAP_KEY: usize = 253;

/// A ConfigMap `data` key: letters, digits, `-`, `_` and `.`, as the API
/// server requires, and not `.` or `..`.
pub(super) fn configmap_key(key: &str) -> Result<String, ToolError> {
    let valid = !key.is_empty()
        && key.len() <= MAX_CONFIGMAP_KEY
        && key != "."
        && key != ".."
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if valid {
        Ok(key.to_string())
    } else {
        Err(ToolError::InvalidArguments {
            message: format!("`key` {key:?} is not a valid ConfigMap key"),
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ScaleWorkloadInput {
    /// The context's name, as `list_contexts` reports it.
    pub(super) context: String,
    /// The workload's namespace.
    pub(super) namespace: String,
    /// The workload's kind.
    pub(super) kind: ScalableKind,
    /// The workload's name.
    pub(super) name: String,
    /// The replica count to set.
    pub(super) replicas: u32,
}

/// A replica count the API accepts: `spec.replicas` is an `int32`.
pub(super) fn replicas(requested: u32) -> Result<i32, ToolError> {
    i32::try_from(requested).map_err(|_| ToolError::InvalidArguments {
        message: format!("`replicas` must be at most {}", i32::MAX),
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct RolloutInput {
    /// The context's name, as `list_contexts` reports it.
    pub(super) context: String,
    /// The workload's namespace.
    pub(super) namespace: String,
    /// The workload's kind.
    pub(super) kind: RolloutKind,
    /// The workload's name.
    pub(super) name: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct SetRolloutPausedInput {
    /// The context's name, as `list_contexts` reports it.
    pub(super) context: String,
    /// The Deployment's namespace.
    pub(super) namespace: String,
    /// The Deployment's name.
    pub(super) name: String,
    /// `true` to pause the rollout, `false` to resume it.
    pub(super) paused: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct DeletePodsInput {
    /// The context's name, as `list_contexts` reports it.
    pub(super) context: String,
    /// The namespace all the Pods are in.
    pub(super) namespace: String,
    /// The Pods to delete, by name: 1 to 10, no repeats.
    pub(super) names: Vec<String>,
}

/// `delete_pods`' names: 1 to [`MCP_DELETE_PODS_MAX`], each a valid name,
/// none twice - the dialog lists each one exactly as it will be deleted.
pub(super) fn pod_names(names: &[String]) -> Result<Vec<ObjectName>, ToolError> {
    if names.is_empty() || names.len() > MCP_DELETE_PODS_MAX {
        return Err(ToolError::InvalidArguments {
            message: format!("`names` must list 1 to {MCP_DELETE_PODS_MAX} Pods"),
        });
    }
    let mut parsed: Vec<ObjectName> = Vec::with_capacity(names.len());
    for name in names {
        let name = ObjectName::parse("names", name)?;
        if parsed.contains(&name) {
            return Err(ToolError::InvalidArguments {
                message: format!("`names` lists {:?} twice", name.as_str()),
            });
        }
        parsed.push(name);
    }
    Ok(parsed)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct CronJobInput {
    /// The context's name, as `list_contexts` reports it.
    pub(super) context: String,
    /// The CronJob's namespace.
    pub(super) namespace: String,
    /// The CronJob's name.
    pub(super) name: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct SetCronJobSuspendedInput {
    /// The context's name, as `list_contexts` reports it.
    pub(super) context: String,
    /// The CronJob's namespace.
    pub(super) namespace: String,
    /// The CronJob's name.
    pub(super) name: String,
    /// `true` to suspend the schedule, `false` to resume it.
    pub(super) suspended: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_kind_outside_a_tools_allowlist_does_not_parse() {
        let scale = |kind: &str| {
            serde_json::from_value::<ScaleWorkloadInput>(json!({
                "context": "dev", "namespace": "a", "kind": kind, "name": "x", "replicas": 1,
            }))
        };
        assert!(scale("Deployment").is_ok());
        for kind in ["DaemonSet", "Secret", "deployment", "CronJob"] {
            assert!(scale(kind).is_err(), "{kind}");
        }
        let rollout = |kind: &str| {
            serde_json::from_value::<RolloutInput>(json!({
                "context": "dev", "namespace": "a", "kind": kind, "name": "x",
            }))
        };
        assert!(rollout("DaemonSet").is_ok());
        assert!(rollout("ReplicaSet").is_err());
    }

    #[test]
    fn kindless_tools_refuse_a_kind_argument() {
        let secret = serde_json::from_value::<SetConfigMapValueInput>(json!({
            "context": "dev", "namespace": "a", "name": "x", "key": "k", "value": "v",
            "kind": "Secret",
        }));
        assert!(secret.is_err());
        let statefulset = serde_json::from_value::<SetRolloutPausedInput>(json!({
            "context": "dev", "namespace": "a", "name": "x", "paused": true,
            "kind": "StatefulSet",
        }));
        assert!(statefulset.is_err());
    }

    #[test]
    fn delete_pods_takes_one_to_ten_distinct_names() {
        let names = |n: usize| (0..n).map(|i| format!("pod-{i}")).collect::<Vec<_>>();
        assert_eq!(pod_names(&names(10)).unwrap().len(), 10);
        assert!(pod_names(&names(11)).is_err());
        assert!(pod_names(&[]).is_err());
        assert!(pod_names(&["a".into(), "a".into()]).is_err());
        assert!(pod_names(&["a".into(), "../b".into()]).is_err());
    }

    #[test]
    fn configmap_keys_follow_the_api_rule() {
        for key in ["LOG_LEVEL", "app.properties", "a-b_c.d"] {
            assert_eq!(configmap_key(key).unwrap(), key);
        }
        for key in ["", ".", "..", "a/b", "a b", "ключ"] {
            assert!(configmap_key(key).is_err(), "{key:?}");
        }
    }

    #[test]
    fn replicas_fit_an_int32() {
        assert_eq!(replicas(3), Ok(3));
        assert!(replicas(u32::MAX).is_err());
    }
}
