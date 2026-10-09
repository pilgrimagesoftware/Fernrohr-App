//! Turning what a read tool found into its result.

use crate::mcp::tools::ToolOutput;
use serde_json::{Map, Value};

/// A result whose content is `value`, which the tools always build as an
/// object; anything else is wrapped as `{"value": ...}`.
pub(super) fn output(value: Value) -> ToolOutput {
    match value {
        Value::Object(content) => ToolOutput::new(content),
        other => ToolOutput::new(Map::from_iter([("value".to_string(), other)])),
    }
}
