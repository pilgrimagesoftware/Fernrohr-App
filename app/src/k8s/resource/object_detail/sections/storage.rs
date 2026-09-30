//! A PersistentVolumeClaim's section: whether it is bound, to what, and how
//! much it asked for and got. The volume and storage class are references.

use super::super::model::{ObjectField, ObjectSection};
use super::common::{non_empty, quantities};
use crate::k8s::object_ref::ObjectRef;
use k8s_openapi::api::core::v1::PersistentVolumeClaim;

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
    if let Some(class) = non_empty(spec.and_then(|spec| spec.storage_class_name.as_deref())) {
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
    if let Some(capacity) = quantities(status.and_then(|status| status.capacity.as_ref())) {
        fields.push(ObjectField::new("Capacity", capacity));
    }
    let requested = spec.and_then(|spec| spec.resources.as_ref()?.requests.as_ref());
    if let Some(requested) = quantities(requested) {
        fields.push(ObjectField::new("Requested", requested));
    }
    let modes = spec
        .and_then(|spec| spec.access_modes.as_ref())
        .filter(|modes| !modes.is_empty());
    if let Some(modes) = modes {
        fields.push(ObjectField::text("Access Modes", modes.join(", ")));
    }
    if let Some(mode) = non_empty(spec.and_then(|spec| spec.volume_mode.as_deref())) {
        fields.push(ObjectField::text("Volume Mode", mode));
    }

    vec![ObjectSection::new("Claim", fields)]
}
