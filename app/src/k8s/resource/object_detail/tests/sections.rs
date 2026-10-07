//! Node, ConfigMap, PersistentVolumeClaim and ServiceAccount sections
//! (`resource-links` 6.1 and 6.3's ServiceAccount).

use super::fixtures::{kind, nodes, object};
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::object_detail::model::{FieldValue, ObjectField, ObjectSection};
use crate::k8s::resource::object_detail::sections::sections_for;
use crate::ui::detail::BadgeTone;
use crate::ui::style::Tone;
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

    assert_eq!(
        field(&sections, "Status").value,
        FieldValue::Status {
            text: "Bound".into(),
            tone: Tone::Good,
        }
    );
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

/// `standard-resource-panels` 3.5: every kind the object-detail spec lists
/// gets its own sections, from a minimal object of that kind - and those
/// sections say something. An object that fails to deserialize as its kind
/// would get no sections at all, so this also catches a projection the API's
/// required fields would trip.
#[test]
fn every_listed_kind_gets_sections_with_fields() {
    let template = json!({ "spec": { "containers": [] } });
    let selector = json!({ "matchLabels": { "app": "web" } });
    let cases = [
        (
            "",
            "Node",
            false,
            json!({ "status": { "capacity": { "cpu": "1" } } }),
        ),
        ("", "ConfigMap", true, json!({ "data": { "key": "value" } })),
        (
            "",
            "Secret",
            true,
            json!({ "data": { "token": "c2VjcmV0" } }),
        ),
        (
            "",
            "PersistentVolumeClaim",
            true,
            json!({ "status": { "phase": "Pending" } }),
        ),
        (
            "",
            "ServiceAccount",
            true,
            json!({ "automountServiceAccountToken": false }),
        ),
        (
            "apps",
            "ReplicaSet",
            true,
            json!({ "spec": { "selector": selector } }),
        ),
        (
            "apps",
            "Deployment",
            true,
            json!({ "spec": { "selector": selector, "template": template } }),
        ),
        (
            "apps",
            "StatefulSet",
            true,
            json!({ "spec": { "selector": selector, "template": template, "serviceName": "web" } }),
        ),
        (
            "apps",
            "DaemonSet",
            true,
            json!({ "spec": { "selector": selector, "template": template } }),
        ),
        (
            "batch",
            "Job",
            true,
            json!({ "spec": { "template": template } }),
        ),
        (
            "batch",
            "CronJob",
            true,
            json!({ "spec": { "schedule": "@daily", "jobTemplate": { "spec": { "template": template } } } }),
        ),
        (
            "",
            "Service",
            true,
            json!({ "spec": { "type": "ClusterIP" } }),
        ),
        (
            "networking.k8s.io",
            "Ingress",
            true,
            json!({ "spec": { "ingressClassName": "nginx" } }),
        ),
        (
            "",
            "Endpoints",
            true,
            json!({ "subsets": [{ "addresses": [{ "ip": "10.0.0.1" }] }] }),
        ),
        (
            "discovery.k8s.io",
            "EndpointSlice",
            true,
            json!({ "addressType": "IPv4", "endpoints": [] }),
        ),
        (
            "networking.k8s.io",
            "NetworkPolicy",
            true,
            json!({ "spec": { "podSelector": {} } }),
        ),
        (
            "",
            "Namespace",
            false,
            json!({ "status": { "phase": "Active" } }),
        ),
        (
            "",
            "PersistentVolume",
            false,
            json!({ "spec": { "capacity": { "storage": "1Gi" } } }),
        ),
        (
            "storage.k8s.io",
            "StorageClass",
            false,
            json!({ "provisioner": "example.com/csi" }),
        ),
        (
            "rbac.authorization.k8s.io",
            "Role",
            true,
            json!({ "rules": [] }),
        ),
        (
            "rbac.authorization.k8s.io",
            "ClusterRole",
            false,
            json!({ "rules": [] }),
        ),
        (
            "rbac.authorization.k8s.io",
            "RoleBinding",
            true,
            json!({ "roleRef": { "apiGroup": "rbac.authorization.k8s.io", "kind": "Role", "name": "r" } }),
        ),
        (
            "rbac.authorization.k8s.io",
            "ClusterRoleBinding",
            false,
            json!({ "roleRef": { "apiGroup": "rbac.authorization.k8s.io", "kind": "ClusterRole", "name": "r" } }),
        ),
    ];

    for (group, kind_name, namespaced, body) in cases {
        let api_version = if group.is_empty() {
            "v1".to_string()
        } else {
            format!("{group}/v1")
        };
        let mut json = json!({
            "apiVersion": api_version,
            "kind": kind_name,
            "metadata": { "name": "example" },
        });
        if namespaced {
            json["metadata"]["namespace"] = json!("staging");
        }
        for (key, value) in body.as_object().expect("an object body") {
            json[key] = value.clone();
        }

        let sections = sections_for(&kind(group, "v1", kind_name, namespaced), &object(json));

        assert!(!sections.is_empty(), "{kind_name} gets sections");
        assert!(
            sections.iter().any(|section| !section.fields.is_empty()),
            "{kind_name}'s sections show at least one field: {sections:?}"
        );
    }
}

/// A kind outside the list gets no sections - Overview only.
#[test]
fn an_unlisted_kind_gets_no_sections() {
    let lease = object(json!({
        "apiVersion": "coordination.k8s.io/v1",
        "kind": "Lease",
        "metadata": { "name": "example", "namespace": "kube-system" },
    }));

    assert!(sections_for(&kind("coordination.k8s.io", "v1", "Lease", true), &lease).is_empty());
}
