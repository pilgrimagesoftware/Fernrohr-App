//! Node, ConfigMap, PersistentVolumeClaim and ServiceAccount sections
//! (`resource-links` 6.1 and 6.3's ServiceAccount).

use super::fixtures::{kind, nodes, object};
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::object_detail::model::{FieldValue, ObjectField, ObjectSection};
use crate::k8s::resource::object_detail::sections::sections_for;
use crate::ui::detail::BadgeTone;
use serde_json::json;

pub(super) fn field<'a>(sections: &'a [ObjectSection], label: &str) -> &'a ObjectField {
    sections
        .iter()
        .flat_map(|section| &section.fields)
        .find(|field| field.label == label)
        .unwrap_or_else(|| panic!("no {label} row in {sections:?}"))
}

#[test]
fn a_node_shows_addresses_resources_conditions_info_and_taints() {
    let node = object(json!({
        "apiVersion": "v1",
        "kind": "Node",
        "metadata": { "name": "node-a" },
        "spec": {
            "podCIDR": "10.244.0.0/24",
            "taints": [{ "key": "dedicated", "value": "gpu", "effect": "NoSchedule" }],
        },
        "status": {
            "addresses": [
                { "type": "InternalIP", "address": "10.0.0.1" },
                { "type": "Hostname", "address": "node-a" },
            ],
            "capacity": { "cpu": "4", "memory": "16Gi" },
            "allocatable": { "cpu": "3800m", "memory": "15Gi" },
            "conditions": [
                { "type": "MemoryPressure", "status": "False" },
                { "type": "DiskPressure", "status": "True" },
                { "type": "Ready", "status": "True" },
            ],
            "nodeInfo": {
                "machineID": "m", "systemUUID": "s", "bootID": "b",
                "kernelVersion": "6.8.0", "osImage": "Ubuntu 24.04",
                "containerRuntimeVersion": "containerd://1.7", "kubeletVersion": "v1.33.1",
                "kubeProxyVersion": "", "operatingSystem": "linux", "architecture": "arm64",
            },
        },
    }));

    let sections = sections_for(&nodes(), &node);

    assert_eq!(sections[0].title, "Node");
    assert_eq!(
        field(&sections, "Addresses").value.text(),
        "InternalIP: 10.0.0.1, Hostname: node-a"
    );
    assert_eq!(
        field(&sections, "Capacity").value.text(),
        "cpu=4, memory=16Gi"
    );
    assert_eq!(
        field(&sections, "Allocatable").value.text(),
        "cpu=3800m, memory=15Gi"
    );
    assert_eq!(
        field(&sections, "Conditions").value,
        FieldValue::Badges(vec![
            ("MemoryPressure".into(), BadgeTone::Good),
            ("DiskPressure".into(), BadgeTone::Warning),
            ("Ready".into(), BadgeTone::Good),
        ]),
        "a pressure condition is good news when it doesn't hold"
    );
    assert!(
        field(&sections, "Node Info")
            .value
            .text()
            .contains("Kubelet: v1.33.1")
    );
    assert_eq!(field(&sections, "Pod CIDR").value.text(), "10.244.0.0/24");
    assert_eq!(
        field(&sections, "Taints").value.text(),
        "dedicated=gpu: NoSchedule"
    );
}

#[test]
fn a_config_map_shows_each_key_and_value() {
    let config_map = object(json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": { "name": "app-config", "namespace": "staging" },
        "data": { "LOG_LEVEL": "debug", "app.yaml": "port: 8080\nregion: eu\n" },
    }));

    let sections = sections_for(&kind("", "v1", "ConfigMap", true), &config_map);

    assert_eq!(
        field(&sections, "Data").value,
        FieldValue::KeyValues(vec![
            ("LOG_LEVEL".into(), "debug".into()),
            ("app.yaml".into(), "port: 8080\nregion: eu\n".into()),
        ])
    );
}

#[test]
fn a_bound_claim_references_its_volume_and_storage_class() {
    let claim = object(json!({
        "apiVersion": "v1",
        "kind": "PersistentVolumeClaim",
        "metadata": { "name": "app-data", "namespace": "staging" },
        "spec": {
            "accessModes": ["ReadWriteOnce"],
            "storageClassName": "fast-ssd",
            "volumeName": "pvc-0f3a",
            "volumeMode": "Filesystem",
            "resources": { "requests": { "storage": "10Gi" } },
        },
        "status": { "phase": "Bound", "capacity": { "storage": "10Gi" } },
    }));

    let sections = sections_for(&kind("", "v1", "PersistentVolumeClaim", true), &claim);

    assert_eq!(field(&sections, "Status").value.text(), "Bound");
    assert_eq!(
        field(&sections, "Volume").value,
        FieldValue::References {
            targets: vec![ObjectRef::cluster_scoped(
                "",
                "PersistentVolume",
                "pvc-0f3a"
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
    assert_eq!(field(&sections, "Capacity").value.text(), "storage=10Gi");
    assert_eq!(field(&sections, "Requested").value.text(), "storage=10Gi");
    assert_eq!(
        field(&sections, "Access Modes").value.text(),
        "ReadWriteOnce"
    );
}

#[test]
fn a_service_account_references_its_secrets() {
    let account = object(json!({
        "apiVersion": "v1",
        "kind": "ServiceAccount",
        "metadata": { "name": "api", "namespace": "staging" },
        "secrets": [{ "name": "api-token" }],
        "imagePullSecrets": [{ "name": "registry" }],
        "automountServiceAccountToken": false,
    }));

    let sections = sections_for(&kind("", "v1", "ServiceAccount", true), &account);

    assert_eq!(
        field(&sections, "Secrets").value,
        FieldValue::References {
            targets: vec![ObjectRef::core("Secret", "staging", "api-token")],
            qualified: false,
        }
    );
    assert_eq!(
        field(&sections, "Image Pull Secrets").value,
        FieldValue::References {
            targets: vec![ObjectRef::core("Secret", "staging", "registry")],
            qualified: false,
        }
    );
    assert_eq!(field(&sections, "Automount Token").value.text(), "No");
}

/// A kind with no dedicated sections, and an object that doesn't deserialize
/// as its kind, both get Overview only.
#[test]
fn other_kinds_and_malformed_objects_get_no_sections() {
    let widget = object(json!({
        "apiVersion": "example.com/v1",
        "kind": "Widget",
        "metadata": { "name": "w" },
    }));
    assert!(sections_for(&kind("example.com", "v1", "Widget", false), &widget).is_empty());

    let malformed = object(json!({
        "apiVersion": "v1",
        "kind": "Node",
        "metadata": { "name": "n" },
        "status": { "capacity": "not a map" },
    }));
    assert!(sections_for(&nodes(), &malformed).is_empty());
}
