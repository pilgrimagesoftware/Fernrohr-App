//! ConfigMap and Secret sections. A ConfigMap shows its data; a Secret shows
//! its type and each key's size, and never a value - it is read from the
//! object `redact` has already replaced the values in.

use super::super::model::{FieldValue, ObjectField, ObjectSection};
use crate::k8s::object_ref::ObjectRef;
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

/// The Secret's type, its `data` keys with their sizes - revealable one at a
/// time - and any `stringData` keys (write-only, so nothing to reveal). Read
/// from the object `redact` already replaced the values in: the sizes come from
/// its placeholders.
pub(super) fn secret(object: &DynamicObject) -> Vec<ObjectSection> {
    let mut fields = Vec::new();
    if let Some(type_) = object.data.get("type").and_then(|value| value.as_str()) {
        fields.push(ObjectField::text("Type", type_));
    }
    let entries = |key: &str| -> Vec<(String, usize)> {
        object
            .data
            .get(key)
            .and_then(|value| value.as_object())
            .into_iter()
            .flatten()
            .map(|(name, placeholder)| {
                let size = placeholder
                    .as_str()
                    .and_then(super::super::redact::placeholder_size)
                    .unwrap_or_default();
                (name.clone(), size)
            })
            .collect()
    };
    let data = entries("data");
    if !data.is_empty() {
        let secret = ObjectRef::core(
            "Secret",
            object.metadata.namespace.as_deref().unwrap_or_default(),
            object.metadata.name.as_deref().unwrap_or_default(),
        );
        fields.push(ObjectField::new(
            "Data",
            FieldValue::SecretKeys { secret, keys: data },
        ));
    }
    let string_data: Vec<String> = entries("stringData")
        .into_iter()
        .map(|(key, size)| format!("{key}: {size} bytes"))
        .collect();
    if !string_data.is_empty() {
        fields.push(ObjectField::new(
            "String Data",
            FieldValue::Lines(string_data),
        ));
    }
    vec![ObjectSection::new("Secret", fields)]
}
