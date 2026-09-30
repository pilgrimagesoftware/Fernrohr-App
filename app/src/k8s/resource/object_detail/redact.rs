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
/// as it is.
pub(super) fn redact(kind: &DiscoveredKind, object: &mut DynamicObject) {
    if !(kind.gvk.group.is_empty() && kind.gvk.kind == "Secret") {
        return;
    }
    if let Some(Value::Object(data)) = object.data.get_mut("data") {
        for value in data.values_mut() {
            let size = value.as_str().map(base64_decoded_len).unwrap_or_default();
            *value = Value::String(placeholder(size));
        }
    }
    if let Some(Value::Object(data)) = object.data.get_mut("stringData") {
        for value in data.values_mut() {
            let size = value.as_str().map(str::len).unwrap_or_default();
            *value = Value::String(placeholder(size));
        }
    }
    if let Some(annotations) = object.metadata.annotations.as_mut()
        && let Some(applied) = annotations.get_mut(LAST_APPLIED_ANNOTATION)
    {
        *applied = placeholder(applied.len());
    }
}

/// What a value is replaced with.
pub(super) fn placeholder(size: usize) -> String {
    let unit = if size == 1 { "byte" } else { "bytes" };
    format!("<redacted: {size} {unit}>")
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
    fn decoded_length_counts_padding() {
        assert_eq!(base64_decoded_len("aGVsbG8="), 5, "hello");
        assert_eq!(base64_decoded_len("aGk="), 2, "hi");
        assert_eq!(base64_decoded_len("YWJj"), 3, "abc");
        assert_eq!(base64_decoded_len(""), 0);
    }
}
