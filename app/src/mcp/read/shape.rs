//! Turning what a read tool found into its result.

use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::object_detail::redact_secret_values;
use crate::mcp::error::ToolError;
use crate::mcp::tools::ToolOutput;
use kube::api::{DynamicObject, TypeMeta};
use serde_json::{Map, Value};

/// A result whose content is `value`, which the tools always build as an
/// object; anything else is wrapped as `{"value": ...}`.
pub(super) fn output(value: Value) -> ToolOutput {
    match value {
        Value::Object(content) => ToolOutput::new(content),
        other => ToolOutput::new(Map::from_iter([("value".to_string(), other)])),
    }
}

/// One object of `kind` as a client receives it: its `apiVersion` and `kind`
/// filled in (a list's items lack them), `managedFields` dropped as noise, and
/// a Secret's values replaced by their sizes - the detail panel's rule, so an
/// agent never sees what the user can't see without asking.
pub(super) fn object_json(
    kind: &DiscoveredKind,
    mut object: DynamicObject,
) -> Result<Value, ToolError> {
    object.types = Some(TypeMeta {
        api_version: kind.gvk.api_version(),
        kind: kind.gvk.kind.clone(),
    });
    object.metadata.managed_fields = None;
    redact_secret_values(kind, &mut object);
    serde_json::to_value(&object).map_err(|_| ToolError::Internal)
}
