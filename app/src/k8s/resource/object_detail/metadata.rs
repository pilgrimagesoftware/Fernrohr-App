//! The Overview section every kind gets: what `ObjectMeta` says about the
//! object, with its namespace and owners as references.

use super::model::{FieldValue, ObjectField, ObjectSection};
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::pods::format_age;
use crate::ui::nav::ObjectTarget;
use jiff::Timestamp;
use kube::api::DynamicObject;
use std::collections::BTreeMap;

/// Created, Name, Namespace, Kind, Labels, Annotations and Controlled By, in
/// that order; rows whose source is absent are left out rather than blank.
pub(super) fn overview(
    object: &DynamicObject,
    target: &ObjectTarget,
    now: Timestamp,
) -> ObjectSection {
    let meta = &object.metadata;
    let mut fields = Vec::new();

    if let Some(created) = &meta.creation_timestamp {
        let age_secs = now.duration_since(created.0).as_secs_f64() as i64;
        fields.push(ObjectField::text(
            "Created",
            format!("{} ({})", format_age(age_secs), created.0),
        ));
    }
    fields.push(ObjectField::text(
        "Name",
        meta.name.clone().unwrap_or_else(|| target.name.clone()),
    ));
    let namespace = meta.namespace.as_deref().or(target.namespace.as_deref());
    if let Some(namespace) = namespace.filter(|namespace| !namespace.is_empty()) {
        fields.push(ObjectField::references(
            "Namespace",
            vec![ObjectRef::cluster_scoped("", "Namespace", namespace)],
            false,
        ));
    }
    let gvk = &target.kind.gvk;
    let api_version = if gvk.group.is_empty() {
        gvk.version.clone()
    } else {
        format!("{}/{}", gvk.group, gvk.version)
    };
    fields.push(ObjectField::text(
        "Kind",
        format!("{} ({api_version})", gvk.kind),
    ));
    if let Some(labels) = chips(&meta.labels) {
        fields.push(ObjectField::new("Labels", labels));
    }
    if let Some(annotations) = chips(&meta.annotations) {
        fields.push(ObjectField::new("Annotations", annotations));
    }
    let owners: Vec<ObjectRef> = meta
        .owner_references
        .iter()
        .flatten()
        .map(|owner| ObjectRef::from_owner(owner, namespace))
        .collect();
    if !owners.is_empty() {
        fields.push(ObjectField::references("Controlled By", owners, true));
    }

    ObjectSection::new("Overview", fields)
}

fn chips(map: &Option<BTreeMap<String, String>>) -> Option<FieldValue> {
    let map = map.as_ref().filter(|map| !map.is_empty())?;
    Some(FieldValue::Chips(
        map.iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect(),
    ))
}
