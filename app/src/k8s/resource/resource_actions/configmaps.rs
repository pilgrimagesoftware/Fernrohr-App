//! Setting or removing one key of a ConfigMap's `data`.
//!
//! [`configmap_value`] reads the key's value and the ConfigMap's
//! `resourceVersion`; [`set_configmap_value`] sends a merge patch naming only
//! that key - `null` removes it - and that `resourceVersion`, so the value a
//! caller showed as "old" is the one replaced, or the API refuses with a
//! conflict. No other key, and no other field, is in the patch.

use super::{ActionError, api_for, patch_params};
use crate::k8s::cluster::discovery::DiscoveredKind;
use kube::api::Patch;
use serde_json::{Value, json};

/// A ConfigMap key's value as read, and the version it was read at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigMapValue {
    /// `None` when the key isn't in `data`.
    pub(crate) value: Option<String>,
    resource_version: Option<String>,
}

/// Reads `key` from ConfigMap `name`.
pub(crate) async fn configmap_value(
    client: kube::Client,
    kind: &DiscoveredKind,
    namespace: &str,
    name: &str,
    key: &str,
) -> Result<ConfigMapValue, ActionError> {
    let configmap = api_for(client, kind, Some(namespace)).get(name).await?;
    Ok(ConfigMapValue {
        value: configmap.data["data"][key].as_str().map(str::to_string),
        resource_version: configmap.metadata.resource_version,
    })
}

/// Sets `key` to `value`, or removes it when `value` is `None`, provided the
/// ConfigMap is still as `read` found it.
pub(crate) async fn set_configmap_value(
    client: kube::Client,
    kind: &DiscoveredKind,
    namespace: &str,
    name: &str,
    key: &str,
    value: Option<&str>,
    read: &ConfigMapValue,
) -> Result<(), ActionError> {
    let patch = value_patch(key, value, read.resource_version.as_deref());
    api_for(client, kind, Some(namespace))
        .patch(name, &patch_params(), &Patch::Merge(&patch))
        .await?;
    Ok(())
}

pub(super) fn value_patch(key: &str, value: Option<&str>, resource_version: Option<&str>) -> Value {
    let mut data = serde_json::Map::new();
    data.insert(key.to_string(), value.map_or(Value::Null, Value::from));
    let mut patch = json!({ "data": data });
    if let Some(version) = resource_version {
        patch["metadata"] = json!({ "resourceVersion": version });
    }
    patch
}
