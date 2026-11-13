//! Keeping Secret values out of the panel entirely (`object-detail`'s "Secret
//! values are never shown").
//!
//! Done once, to the fetched object, before the panel stores it - so no render
//! path, the YAML view included, can reach a value, rather than each view
//! remembering to hide them. Each value becomes a placeholder giving its size,
//! which is what the Secret section shows beside each key.

use crate::k8s::cluster::discovery::DiscoveredKind;
use kube::api::DynamicObject;
use serde_json::Value;

/// The annotation `kubectl apply` writes: the whole manifest as last applied,
/// values included.
pub(super) const LAST_APPLIED_ANNOTATION: &str = "kubectl.kubernetes.io/last-applied-configuration";

/// Redacts `object` in place when it is a core Secret; any other kind is left
/// as it is. `agent-mcp`'s read tools apply it to every object they return.
pub(crate) fn redact(kind: &DiscoveredKind, object: &mut DynamicObject) {
    if !(kind.gvk.group.is_empty() && kind.gvk.kind == "Secret") {
        return;
    }
    redact_values(&mut object.data);
    if let Some(annotations) = object.metadata.annotations.as_mut()
        && let Some(applied) = annotations.get_mut(LAST_APPLIED_ANNOTATION)
    {
        *applied = placeholder(applied.len());
    }
}

/// [`redact`] for a Secret given as JSON - one document of a Helm release's
/// manifest (helm-releases D6): its `data`, `stringData` and last-applied
/// annotation, by the same rule.
pub(crate) fn redact_secret_json(secret: &mut Value) {
    redact_values(secret);
    let applied = secret
        .get_mut("metadata")
        .and_then(|metadata| metadata.get_mut("annotations"))
        .and_then(|annotations| annotations.get_mut(LAST_APPLIED_ANNOTATION));
    if let Some(applied) = applied {
        let size = applied.as_str().map(str::len).unwrap_or_default();
        *applied = Value::String(placeholder(size));
    }
}

/// Each `data` and `stringData` value of a Secret's fields, as its size.
fn redact_values(fields: &mut Value) {
    if let Some(Value::Object(data)) = fields.get_mut("data") {
        for value in data.values_mut() {
            let size = value.as_str().map(base64_decoded_len).unwrap_or_default();
            *value = Value::String(placeholder(size));
        }
    }
    if let Some(Value::Object(data)) = fields.get_mut("stringData") {
        for value in data.values_mut() {
            let size = value.as_str().map(str::len).unwrap_or_default();
            *value = Value::String(placeholder(size));
        }
    }
}

/// What a value is replaced with.
pub(super) fn placeholder(size: usize) -> String {
    let unit = if size == 1 { "byte" } else { "bytes" };
    format!("<redacted: {size} {unit}>")
}

/// The size a [`placeholder`] states, or `None` for any other text.
pub(super) fn placeholder_size(text: &str) -> Option<usize> {
    let rest = text.strip_prefix("<redacted: ")?;
    let (size, unit) = rest.split_once(' ')?;
    matches!(unit, "byte>" | "bytes>").then(|| size.parse().ok())?
}

/// The decoded length of a base64 value, from its length and padding alone -
/// the value itself is never decoded.
fn base64_decoded_len(encoded: &str) -> usize {
    let symbols = encoded
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .count();
    let padding = encoded
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'=')
        .count();
    (symbols * 3 / 4).saturating_sub(padding)
}

#[cfg(test)]
mod tests {
    use super::base64_decoded_len;

    #[test]
    fn placeholder_size_reads_back_what_placeholder_wrote() {
        use super::{placeholder, placeholder_size};
        for size in [0, 1, 15, 4096] {
            assert_eq!(placeholder_size(&placeholder(size)), Some(size));
        }
        assert_eq!(placeholder_size("hunter2"), None);
    }

    #[test]
    fn decoded_length_counts_padding() {
        assert_eq!(base64_decoded_len("aGVsbG8="), 5, "hello");
        assert_eq!(base64_decoded_len("aGk="), 2, "hi");
        assert_eq!(base64_decoded_len("YWJj"), 3, "abc");
        assert_eq!(base64_decoded_len(""), 0);
    }
}
