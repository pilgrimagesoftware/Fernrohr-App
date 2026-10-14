//! Label and Annotate: one key set to one value - or removed - on any object,
//! by a merge patch that carries that one key alone, as `kubectl label` and
//! `kubectl annotate` send.

use super::BulkTarget;
use crate::k8s::resource::resource_actions::{ActionFailure, api_for};
use kube::api::Patch;
use serde_json::{Value, json};

/// Which of an object's two string maps to write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MetadataMap {
    Labels,
    Annotations,
}

impl MetadataMap {
    fn field(self) -> &'static str {
        match self {
            Self::Labels => "labels",
            Self::Annotations => "annotations",
        }
    }
}

/// The patch: `key` set to `value`, or - `None` - removed (a merge patch's
/// `null`).
pub(super) fn patch(map: MetadataMap, key: &str, value: Option<&str>) -> Value {
    let value = value.map_or(Value::Null, |value| Value::String(value.to_string()));
    json!({ "metadata": { map.field(): { key: value } } })
}

/// Sets or removes `key` on `target`.
pub(super) async fn set(
    client: kube::Client,
    target: &BulkTarget,
    map: MetadataMap,
    key: &str,
    value: Option<&str>,
) -> Result<(), ActionFailure> {
    api_for(client, &target.kind, target.namespace.as_deref())
        .patch(
            &target.name,
            &crate::k8s::resource::resource_actions::patch_params(),
            &Patch::Merge(&patch(map, key, value)),
        )
        .await
        .map(|_| ())
        .map_err(|error| ActionFailure::from_kube(&error))
}
