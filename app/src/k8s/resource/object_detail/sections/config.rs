//! ConfigMap and Secret sections. A ConfigMap shows its data; a Secret shows
//! its type and each key's size, and never a value - it is read from the
//! object `redact` has already replaced the values in.

use super::super::model::{FieldValue, ObjectField, ObjectSection};
use k8s_openapi::api::core::v1::ConfigMap;
use kube::api::DynamicObject;

pub(super) fn config_map(config_map: &ConfigMap) -> Vec<ObjectSection> {
    let mut fields = Vec::new();
    let data: Vec<(String, String)> = config_map
        .data
        .iter()
        .flatten()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if !data.is_empty() {
        fields.push(ObjectField::new("Data", FieldValue::KeyValues(data)));
    }
    let binary: Vec<String> = config_map
        .binary_data
        .iter()
        .flatten()
        .map(|(key, value)| format!("{key}: {} bytes", value.0.len()))
        .collect();
    if !binary.is_empty() {
        fields.push(ObjectField::new("Binary Data", FieldValue::Lines(binary)));
    }
    if config_map.immutable == Some(true) {
        fields.push(ObjectField::text("Immutable", "Yes"));
    }
    vec![ObjectSection::new("ConfigMap", fields)]
}

/// The Secret's type, and one line per key with the size placeholder
/// `redact` left in place of its value.
pub(super) fn secret(object: &DynamicObject) -> Vec<ObjectSection> {
    let mut fields = Vec::new();
    if let Some(type_) = object.data.get("type").and_then(|value| value.as_str()) {
        fields.push(ObjectField::text("Type", type_));
    }
    for (label, key) in [("Data", "data"), ("String Data", "stringData")] {
        let keys: Vec<String> = object
            .data
            .get(key)
            .and_then(|value| value.as_object())
            .into_iter()
            .flatten()
            .map(|(name, placeholder)| {
                format!("{name}: {}", placeholder.as_str().unwrap_or("<redacted>"))
            })
            .collect();
        if !keys.is_empty() {
            fields.push(ObjectField::new(label, FieldValue::Lines(keys)));
        }
    }
    vec![ObjectSection::new("Secret", fields)]
}
