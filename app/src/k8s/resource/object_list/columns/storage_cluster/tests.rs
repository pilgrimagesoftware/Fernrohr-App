//! Storage, Cluster and Access Control kinds' cells from fixture objects
//! (`standard-resource-panels` 2.4), including malformed objects.

use super::{
    CLUSTER_ROLE_BINDING, NAMESPACE, NODE, PERSISTENT_VOLUME, PERSISTENT_VOLUME_CLAIM,
    ROLE_BINDING, SERVICE_ACCOUNT, STORAGE_CLASS_COLUMNS,
};
use crate::k8s::resource::object_list::columns::{Cell, KindColumns};
use crate::ui::style::Tone;
use kube::api::DynamicObject;
use serde_json::json;

fn cells(columns: &KindColumns, json: serde_json::Value) -> Vec<Cell> {
    let object: DynamicObject = serde_json::from_value(json).expect("a valid object");
    columns.cells_for(&object)
}

#[test]
fn a_bound_claim_shows_status_volume_capacity_modes_and_class() {
    let cells = cells(
        &PERSISTENT_VOLUME_CLAIM,
        json!({
            "apiVersion": "v1", "kind": "PersistentVolumeClaim",
            "metadata": { "name": "data", "namespace": "staging" },
            "spec": { "volumeName": "pvc-0f3a", "storageClassName": "fast-ssd" },
            "status": {
                "phase": "Bound",
                "capacity": { "storage": "10Gi" },
                "accessModes": ["ReadWriteOnce", "ReadOnlyMany"],
            },
        }),
    );
    assert_eq!(
        cells,
        vec![
            Cell::status("Bound", Tone::Good),
            Cell::text("pvc-0f3a"),
            Cell::text("10Gi"),
            Cell::text("RWO,ROX"),
            Cell::text("fast-ssd"),
        ]
    );
}

/// A pending claim has no volume or capacity yet: those cells are empty.
#[test]
fn a_pending_claim_leaves_volume_and_capacity_empty() {
    let cells = cells(
        &PERSISTENT_VOLUME_CLAIM,
        json!({
            "apiVersion": "v1", "kind": "PersistentVolumeClaim",
            "metadata": { "name": "data", "namespace": "staging" },
            "spec": { "storageClassName": "fast-ssd" },
            "status": { "phase": "Pending" },
        }),
    );
    assert_eq!(cells[0], Cell::status("Pending", Tone::Info));
    assert_eq!(cells[1], Cell::Empty);
    assert_eq!(cells[2], Cell::Empty);
}

#[test]
fn a_volume_shows_capacity_modes_policy_status_claim_and_class() {
    let cells = cells(
        &PERSISTENT_VOLUME,
        json!({
            "apiVersion": "v1", "kind": "PersistentVolume",
            "metadata": { "name": "pvc-0f3a" },
            "spec": {
                "capacity": { "storage": "10Gi" },
                "accessModes": ["ReadWriteOncePod"],
                "persistentVolumeReclaimPolicy": "Retain",
                "claimRef": { "namespace": "staging", "name": "data" },
                "storageClassName": "fast-ssd",
            },
            "status": { "phase": "Bound" },
        }),
    );
    assert_eq!(
        cells,
        vec![
            Cell::text("10Gi"),
            Cell::text("RWOP"),
            Cell::text("Retain"),
            Cell::status("Bound", Tone::Good),
            Cell::text("staging/data"),
            Cell::text("fast-ssd"),
        ]
    );
}

/// A StorageClass shows its provisioner and policies, the API's defaults where
/// it leaves them unset.
#[test]
fn a_storage_class_shows_provisioner_and_policies_with_defaults() {
    let cells = cells(
        &STORAGE_CLASS_COLUMNS,
        json!({
            "apiVersion": "storage.k8s.io/v1", "kind": "StorageClass",
            "metadata": { "name": "standard" },
            "provisioner": "rancher.io/local-path",
        }),
    );
    assert_eq!(
        cells,
        vec![
            Cell::text("rancher.io/local-path"),
            Cell::text("Delete"),
            Cell::text("Immediate"),
        ]
    );
}

#[test]
fn a_node_shows_status_roles_version_and_internal_ip() {
    let cells = cells(
        &NODE,
        json!({
            "apiVersion": "v1", "kind": "Node",
            "metadata": { "name": "node-a", "labels": {
                "node-role.kubernetes.io/control-plane": "",
                "node-role.kubernetes.io/worker": "",
                "kubernetes.io/hostname": "node-a",
            } },
            "spec": { "unschedulable": true },
            "status": {
                "conditions": [{ "type": "Ready", "status": "True" }],
                "addresses": [
                    { "type": "Hostname", "address": "node-a" },
                    { "type": "InternalIP", "address": "10.0.0.1" },
                ],
                "nodeInfo": {
                    "machineID": "m", "systemUUID": "s", "bootID": "b",
                    "kernelVersion": "6.8", "osImage": "Ubuntu", "containerRuntimeVersion": "c",
                    "kubeletVersion": "v1.33.1", "kubeProxyVersion": "",
                    "operatingSystem": "linux", "architecture": "arm64",
                },
            },
        }),
    );
    assert_eq!(
        cells,
        vec![
            Cell::status("Ready,SchedulingDisabled", Tone::Warning),
            Cell::text("control-plane,worker"),
            Cell::text("v1.33.1"),
            Cell::text("10.0.0.1"),
        ]
    );
}

/// A Node that isn't Ready says so, and one with no role labels reads
/// `<none>`, as `kubectl get nodes` shows it.
#[test]
fn a_not_ready_node_without_roles() {
    let cells = cells(
        &NODE,
        json!({
            "apiVersion": "v1", "kind": "Node",
            "metadata": { "name": "node-b" },
            "status": { "conditions": [{ "type": "Ready", "status": "False" }] },
        }),
    );
    assert_eq!(cells[0], Cell::status("NotReady", Tone::Bad));
    assert_eq!(cells[1], Cell::text("<none>"));
}

#[test]
fn a_namespace_shows_its_status() {
    let cells = cells(
        &NAMESPACE,
        json!({
            "apiVersion": "v1", "kind": "Namespace",
            "metadata": { "name": "staging" },
            "status": { "phase": "Active" },
        }),
    );
    assert_eq!(cells, vec![Cell::status("Active", Tone::Good)]);
}

#[test]
fn a_service_account_counts_its_secrets() {
    let cells = cells(
        &SERVICE_ACCOUNT,
        json!({
            "apiVersion": "v1", "kind": "ServiceAccount",
            "metadata": { "name": "ci", "namespace": "staging" },
            "secrets": [{ "name": "ci-token" }, { "name": "ci-dockercfg" }],
        }),
    );
    assert_eq!(cells, vec![Cell::Number(2)]);
}

#[test]
fn a_role_binding_shows_its_role_and_subjects() {
    let cells = cells(
        &ROLE_BINDING,
        json!({
            "apiVersion": "rbac.authorization.k8s.io/v1", "kind": "RoleBinding",
            "metadata": { "name": "deployers", "namespace": "staging" },
            "roleRef": { "apiGroup": "rbac.authorization.k8s.io", "kind": "Role", "name": "deployer" },
            "subjects": [
                { "kind": "ServiceAccount", "name": "ci", "namespace": "staging" },
                { "kind": "User", "name": "alice", "apiGroup": "rbac.authorization.k8s.io" },
            ],
        }),
    );
    assert_eq!(
        cells,
        vec![
            Cell::text("Role/deployer"),
            Cell::text("ServiceAccount staging/ci, User alice"),
        ]
    );
}

#[test]
fn a_cluster_role_binding_shows_its_role_and_subjects() {
    let cells = cells(
        &CLUSTER_ROLE_BINDING,
        json!({
            "apiVersion": "rbac.authorization.k8s.io/v1", "kind": "ClusterRoleBinding",
            "metadata": { "name": "admins" },
            "roleRef": { "apiGroup": "rbac.authorization.k8s.io", "kind": "ClusterRole", "name": "cluster-admin" },
            "subjects": [{ "kind": "Group", "name": "system:masters", "apiGroup": "rbac.authorization.k8s.io" }],
        }),
    );
    assert_eq!(
        cells,
        vec![
            Cell::text("ClusterRole/cluster-admin"),
            Cell::text("Group system:masters"),
        ]
    );
}

/// A malformed object - a field the wrong type for its kind - gets every
/// column empty, never a panic, and the list still shows its base columns.
#[test]
fn malformed_objects_get_empty_cells() {
    let node = cells(
        &NODE,
        json!({
            "apiVersion": "v1", "kind": "Node",
            "metadata": { "name": "node-a" },
            "status": { "addresses": "10.0.0.1" },
        }),
    );
    let claim = cells(
        &PERSISTENT_VOLUME_CLAIM,
        json!({
            "apiVersion": "v1", "kind": "PersistentVolumeClaim",
            "metadata": { "name": "data", "namespace": "staging" },
            "spec": { "accessModes": "ReadWriteOnce" },
        }),
    );
    let binding = cells(
        &ROLE_BINDING,
        json!({
            "apiVersion": "rbac.authorization.k8s.io/v1", "kind": "RoleBinding",
            "metadata": { "name": "no-role", "namespace": "staging" },
        }),
    );
    assert_eq!(node, vec![Cell::Empty; 4]);
    assert_eq!(claim, vec![Cell::Empty; 5]);
    assert_eq!(
        binding,
        vec![Cell::Empty; 2],
        "a RoleBinding without its required roleRef"
    );
}
