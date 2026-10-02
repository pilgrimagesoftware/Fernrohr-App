//! PersistentVolume and StorageClass sections (`standard-resource-panels`
//! 3.2). The PersistentVolumeClaim's are in `sections`.

use super::fixtures::{kind, object};
use super::sections::field;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::object_detail::model::FieldValue;
use crate::k8s::resource::object_detail::sections::sections_for;
use serde_json::json;

fn volumes() -> DiscoveredKind {
    kind("", "v1", "PersistentVolume", false)
}

fn storage_classes() -> DiscoveredKind {
    kind("storage.k8s.io", "v1", "StorageClass", false)
}

#[test]
fn a_bound_volume_shows_its_storage_and_references_its_claim_and_class() {
    let volume = object(json!({
        "apiVersion": "v1",
        "kind": "PersistentVolume",
        "metadata": { "name": "pvc-0f3a" },
        "spec": {
            "capacity": { "storage": "10Gi" },
            "accessModes": ["ReadWriteOnce"],
            "persistentVolumeReclaimPolicy": "Delete",
            "storageClassName": "fast-ssd",
            "volumeMode": "Filesystem",
            "claimRef": { "kind": "PersistentVolumeClaim", "namespace": "staging", "name": "app-data" },
            "csi": { "driver": "ebs.csi.aws.com", "volumeHandle": "vol-0abc" },
        },
        "status": { "phase": "Bound" },
    }));

    let sections = sections_for(&volumes(), &volume);

    assert_eq!(sections[0].title, "Volume");
    assert_eq!(field(&sections, "Phase").value.text(), "Bound");
    assert_eq!(field(&sections, "Capacity").value.text(), "storage=10Gi");
    assert_eq!(
        field(&sections, "Access Modes").value.text(),
        "ReadWriteOnce"
    );
    assert_eq!(field(&sections, "Reclaim Policy").value.text(), "Delete");
    assert_eq!(
        field(&sections, "Source").value.text(),
        "CSI ebs.csi.aws.com (vol-0abc)"
    );
    assert_eq!(
        field(&sections, "Claim").value,
        FieldValue::References {
            targets: vec![ObjectRef::core(
                "PersistentVolumeClaim",
                "staging",
                "app-data"
            )],
            qualified: false,
        }
    );
    assert_eq!(
        field(&sections, "Storage Class").value,
        FieldValue::References {
            targets: vec![ObjectRef::cluster_scoped(
                "storage.k8s.io",
                "StorageClass",
                "fast-ssd"
            )],
            qualified: false,
        }
    );
}

/// A source without its own description is named by its field, so no volume
/// shows no source at all.
#[test]
fn a_less_common_volume_source_is_named_by_its_field() {
    let volume = object(json!({
        "apiVersion": "v1",
        "kind": "PersistentVolume",
        "metadata": { "name": "legacy" },
        "spec": {
            "capacity": { "storage": "5Gi" },
            "awsElasticBlockStore": { "volumeID": "vol-1234" },
        },
    }));

    let sections = sections_for(&volumes(), &volume);

    assert_eq!(
        field(&sections, "Source").value.text(),
        "awsElasticBlockStore"
    );
}

#[test]
fn a_storage_class_shows_its_provisioner_and_whether_it_is_the_default() {
    let class = object(json!({
        "apiVersion": "storage.k8s.io/v1",
        "kind": "StorageClass",
        "metadata": {
            "name": "fast-ssd",
            "annotations": { "storageclass.kubernetes.io/is-default-class": "true" },
        },
        "provisioner": "ebs.csi.aws.com",
        "parameters": { "type": "gp3", "iops": "3000" },
        "reclaimPolicy": "Retain",
        "volumeBindingMode": "WaitForFirstConsumer",
        "allowVolumeExpansion": true,
    }));

    let sections = sections_for(&storage_classes(), &class);

    assert_eq!(sections[0].title, "Storage Class");
    assert_eq!(
        field(&sections, "Provisioner").value.text(),
        "ebs.csi.aws.com"
    );
    assert_eq!(
        field(&sections, "Parameters").value,
        FieldValue::KeyValues(vec![
            ("iops".into(), "3000".into()),
            ("type".into(), "gp3".into()),
        ])
    );
    assert_eq!(field(&sections, "Reclaim Policy").value.text(), "Retain");
    assert_eq!(
        field(&sections, "Volume Binding Mode").value.text(),
        "WaitForFirstConsumer"
    );
    assert_eq!(field(&sections, "Allow Expansion").value.text(), "Yes");
    assert_eq!(field(&sections, "Cluster Default").value.text(), "Yes");
}

/// The beta annotation older clusters set still marks the default; a class
/// with neither isn't one.
#[test]
fn the_default_class_annotation_is_read_in_both_forms() {
    let class = |annotations: serde_json::Value| {
        object(json!({
            "apiVersion": "storage.k8s.io/v1",
            "kind": "StorageClass",
            "metadata": { "name": "standard", "annotations": annotations },
            "provisioner": "kubernetes.io/no-provisioner",
        }))
    };

    let beta = class(json!({ "storageclass.beta.kubernetes.io/is-default-class": "true" }));
    let plain = class(json!({}));

    assert_eq!(
        field(&sections_for(&storage_classes(), &beta), "Cluster Default")
            .value
            .text(),
        "Yes"
    );
    assert_eq!(
        field(&sections_for(&storage_classes(), &plain), "Cluster Default")
            .value
            .text(),
        "No"
    );
}
