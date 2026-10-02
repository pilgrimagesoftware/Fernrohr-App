//! Storage kinds' sections. A PersistentVolumeClaim: whether it is bound, to
//! what, and how much it asked for and got. A PersistentVolume: what it holds
//! and where its data lives, its claim and StorageClass as references. A
//! StorageClass: how it provisions, and whether it is the cluster default.

use super::super::model::{FieldValue, ObjectField, ObjectSection};
use super::common::{non_empty, quantities};
use crate::k8s::object_ref::ObjectRef;
use k8s_openapi::api::core::v1::{PersistentVolume, PersistentVolumeClaim, PersistentVolumeSpec};
use k8s_openapi::api::storage::v1::StorageClass;

/// The annotations that mark a StorageClass as the cluster default - the GA
/// one and the beta one older clusters still set.
const DEFAULT_CLASS_ANNOTATIONS: [&str; 2] = [
    "storageclass.kubernetes.io/is-default-class",
    "storageclass.beta.kubernetes.io/is-default-class",
];

/// The `spec` fields of a PersistentVolume that aren't its volume source -
/// whatever else is set names the source.
const VOLUME_SPEC_FIELDS: [&str; 9] = [
    "accessModes",
    "capacity",
    "claimRef",
    "mountOptions",
    "nodeAffinity",
    "persistentVolumeReclaimPolicy",
    "storageClassName",
    "volumeAttributesClassName",
    "volumeMode",
];

fn storage_class(fields: &mut Vec<ObjectField>, class: Option<&str>) {
    if let Some(class) = non_empty(class) {
        fields.push(ObjectField::references(
            "Storage Class",
            vec![ObjectRef::cluster_scoped(
                "storage.k8s.io",
                "StorageClass",
                class,
            )],
            false,
        ));
    }
}

fn access_modes(fields: &mut Vec<ObjectField>, modes: Option<&Vec<String>>) {
    if let Some(modes) = modes.filter(|modes| !modes.is_empty()) {
        fields.push(ObjectField::text("Access Modes", modes.join(", ")));
    }
}

pub(super) fn claim(claim: &PersistentVolumeClaim) -> Vec<ObjectSection> {
    let mut fields = Vec::new();
    let spec = claim.spec.as_ref();
    let status = claim.status.as_ref();

    if let Some(phase) = non_empty(status.and_then(|status| status.phase.as_deref())) {
        fields.push(ObjectField::text("Status", phase));
    }
    if let Some(volume) = non_empty(spec.and_then(|spec| spec.volume_name.as_deref())) {
        fields.push(ObjectField::references(
            "Volume",
            vec![ObjectRef::cluster_scoped("", "PersistentVolume", volume)],
            false,
        ));
    }
    storage_class(
        &mut fields,
        spec.and_then(|spec| spec.storage_class_name.as_deref()),
    );
    if let Some(capacity) = quantities(status.and_then(|status| status.capacity.as_ref())) {
        fields.push(ObjectField::new("Capacity", capacity));
    }
    let requested = spec.and_then(|spec| spec.resources.as_ref()?.requests.as_ref());
    if let Some(requested) = quantities(requested) {
        fields.push(ObjectField::new("Requested", requested));
    }
    access_modes(
        &mut fields,
        spec.and_then(|spec| spec.access_modes.as_ref()),
    );
    if let Some(mode) = non_empty(spec.and_then(|spec| spec.volume_mode.as_deref())) {
        fields.push(ObjectField::text("Volume Mode", mode));
    }

    vec![ObjectSection::new("Claim", fields)]
}

/// Where a volume's data lives: the common sources described, any other named
/// by its field (`awsElasticBlockStore`, `rbd`, ...).
fn volume_source(spec: &PersistentVolumeSpec) -> Option<String> {
    if let Some(csi) = &spec.csi {
        return Some(format!("CSI {} ({})", csi.driver, csi.volume_handle));
    }
    if let Some(host) = &spec.host_path {
        return Some(format!("HostPath {}", host.path));
    }
    if let Some(local) = &spec.local {
        return Some(format!("Local {}", local.path));
    }
    if let Some(nfs) = &spec.nfs {
        return Some(format!("NFS {}:{}", nfs.server, nfs.path));
    }
    let value = serde_json::to_value(spec).ok()?;
    value
        .as_object()?
        .keys()
        .find(|key| !VOLUME_SPEC_FIELDS.contains(&key.as_str()))
        .cloned()
}

pub(super) fn volume(volume: &PersistentVolume) -> Vec<ObjectSection> {
    let mut fields = Vec::new();
    let spec = volume.spec.as_ref();

    if let Some(phase) = non_empty(
        volume
            .status
            .as_ref()
            .and_then(|status| status.phase.as_deref()),
    ) {
        fields.push(ObjectField::text("Phase", phase));
    }
    if let Some(capacity) = quantities(spec.and_then(|spec| spec.capacity.as_ref())) {
        fields.push(ObjectField::new("Capacity", capacity));
    }
    access_modes(
        &mut fields,
        spec.and_then(|spec| spec.access_modes.as_ref()),
    );
    if let Some(policy) =
        non_empty(spec.and_then(|spec| spec.persistent_volume_reclaim_policy.as_deref()))
    {
        fields.push(ObjectField::text("Reclaim Policy", policy));
    }
    if let Some(source) = spec.and_then(volume_source) {
        fields.push(ObjectField::text("Source", source));
    }
    let claim = spec
        .and_then(|spec| spec.claim_ref.as_ref())
        .and_then(|claim| {
            let name = non_empty(claim.name.as_deref())?;
            let namespace = non_empty(claim.namespace.as_deref())?;
            Some(ObjectRef::core("PersistentVolumeClaim", namespace, name))
        });
    if let Some(claim) = claim {
        fields.push(ObjectField::references("Claim", vec![claim], false));
    }
    storage_class(
        &mut fields,
        spec.and_then(|spec| spec.storage_class_name.as_deref()),
    );
    if let Some(mode) = non_empty(spec.and_then(|spec| spec.volume_mode.as_deref())) {
        fields.push(ObjectField::text("Volume Mode", mode));
    }

    vec![ObjectSection::new("Volume", fields)]
}

pub(super) fn class(class: &StorageClass) -> Vec<ObjectSection> {
    let mut fields = vec![ObjectField::text("Provisioner", class.provisioner.clone())];

    let parameters: Vec<(String, String)> = class
        .parameters
        .iter()
        .flatten()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if !parameters.is_empty() {
        fields.push(ObjectField::new(
            "Parameters",
            FieldValue::KeyValues(parameters),
        ));
    }
    if let Some(policy) = non_empty(class.reclaim_policy.as_deref()) {
        fields.push(ObjectField::text("Reclaim Policy", policy));
    }
    if let Some(mode) = non_empty(class.volume_binding_mode.as_deref()) {
        fields.push(ObjectField::text("Volume Binding Mode", mode));
    }
    if let Some(expansion) = class.allow_volume_expansion {
        fields.push(ObjectField::text(
            "Allow Expansion",
            if expansion { "Yes" } else { "No" },
        ));
    }
    let annotations = class.metadata.annotations.as_ref();
    let default = DEFAULT_CLASS_ANNOTATIONS.iter().any(|annotation| {
        annotations
            .and_then(|annotations| annotations.get(*annotation))
            .is_some_and(|value| value == "true")
    });
    fields.push(ObjectField::text(
        "Cluster Default",
        if default { "Yes" } else { "No" },
    ));

    vec![ObjectSection::new("Storage Class", fields)]
}
