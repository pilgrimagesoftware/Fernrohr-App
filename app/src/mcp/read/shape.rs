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

/// The leading `items` whose serialized sizes together fit `budget`, and
/// whether any were left out. Whole items only: a cut object would read as a
/// different one.
pub(super) fn within_budget(items: Vec<Value>, budget: usize) -> (Vec<Value>, bool) {
    let total = items.len();
    let mut used = 0;
    let kept: Vec<Value> = items
        .into_iter()
        .take_while(|item| {
            used += serde_json::to_vec(item).map_or(usize::MAX, |bytes| bytes.len() + 1);
            used <= budget
        })
        .collect();
    let truncated = kept.len() < total;
    (kept, truncated)
}

/// The newest at most `limit` bytes of `log`, as text starting at a line (or,
/// for one line longer than `limit`, at a character), and whether older bytes
/// were dropped.
pub(super) fn newest_bytes(log: &[u8], limit: usize) -> (String, bool) {
    if log.len() <= limit {
        return (String::from_utf8_lossy(log).into_owned(), false);
    }
    let tail = &log[log.len() - limit..];
    let start = match tail.iter().position(|&byte| byte == b'\n') {
        Some(newline) if newline + 1 < tail.len() => newline + 1,
        // No line starts inside the window: skip a partial character instead.
        _ => tail
            .iter()
            .position(|&byte| byte & 0b1100_0000 != 0b1000_0000)
            .unwrap_or(tail.len()),
    };
    (String::from_utf8_lossy(&tail[start..]).into_owned(), true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn items_past_the_budget_are_left_out_whole() {
        let items: Vec<Value> = (0..10)
            .map(|n| json!({"n": n, "pad": "x".repeat(90)}))
            .collect();
        let one = serde_json::to_vec(&items[0]).unwrap().len() + 1;
        let (kept, truncated) = within_budget(items.clone(), one * 3 + one / 2);
        assert_eq!(kept, items[..3]);
        assert!(truncated);
        let (kept, truncated) = within_budget(items.clone(), usize::MAX);
        assert_eq!(kept.len(), 10);
        assert!(!truncated);
    }

    #[test]
    fn the_newest_log_lines_are_kept_from_a_line_start() {
        let log = b"first line\nsecond line\nthird\n";
        assert_eq!(
            newest_bytes(log, 100),
            (String::from_utf8_lossy(log).into(), false)
        );
        let (kept, truncated) = newest_bytes(log, 16);
        assert_eq!(kept, "third\n");
        assert!(truncated);
        assert!(kept.len() <= 16);
    }

    #[test]
    fn a_long_line_is_cut_at_a_character_not_mid_byte() {
        let log = "ééééé".as_bytes();
        let (kept, truncated) = newest_bytes(log, 5);
        assert!(truncated);
        assert_eq!(kept, "éé");
    }
}
