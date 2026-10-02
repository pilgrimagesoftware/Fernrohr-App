//! Config and Network kinds' cells from fixture objects
//! (`standard-resource-panels` 2.3).

use super::{CONFIG_MAP, ENDPOINT_SLICE, ENDPOINTS, INGRESS, NETWORK_POLICY, SECRET, SERVICE};
use crate::k8s::resource::object_list::columns::{Cell, KindColumns};
use kube::api::DynamicObject;
use serde_json::json;

fn cells(columns: &KindColumns, json: serde_json::Value) -> Vec<Cell> {
    let object: DynamicObject = serde_json::from_value(json).expect("a valid object");
    columns.cells_for(&object)
}

#[test]
fn a_config_map_counts_its_data_keys() {
    let cells = cells(
        &CONFIG_MAP,
        json!({
            "apiVersion": "v1", "kind": "ConfigMap",
            "metadata": { "name": "app", "namespace": "staging" },
            "data": { "a": "1", "b": "2" },
            "binaryData": { "logo.png": "iVBORw0K" },
        }),
    );
    assert_eq!(cells, vec![Cell::Number(3)], "text and binary keys alike");
}

/// A Secret's type and key count. The count reads the keys, not the values,
/// so it holds for a value that isn't valid base64.
#[test]
fn a_secret_shows_its_type_and_key_count() {
    let cells = cells(
        &SECRET,
        json!({
            "apiVersion": "v1", "kind": "Secret",
            "metadata": { "name": "tls", "namespace": "staging" },
            "type": "kubernetes.io/tls",
            "data": { "tls.crt": "not-base64!", "tls.key": "LS0t" },
        }),
    );
    assert_eq!(
        cells,
        vec![Cell::text("kubernetes.io/tls"), Cell::Number(2)]
    );
}

/// The spec's scenario: a Services list shows type, cluster IP, external IP
/// and ports.
#[test]
fn a_service_shows_type_cluster_ip_external_ip_and_ports() {
    let cells = cells(
        &SERVICE,
        json!({
            "apiVersion": "v1", "kind": "Service",
            "metadata": { "name": "web", "namespace": "staging" },
            "spec": {
                "type": "LoadBalancer",
                "clusterIP": "10.96.0.10",
                "ports": [
                    { "port": 80, "protocol": "TCP", "nodePort": 30080 },
                    { "port": 53, "protocol": "UDP" },
                ],
            },
            "status": { "loadBalancer": { "ingress": [{ "ip": "198.51.100.4" }] } },
        }),
    );
    assert_eq!(
        cells,
        vec![
            Cell::text("LoadBalancer"),
            Cell::text("10.96.0.10"),
            Cell::text("198.51.100.4"),
            Cell::text("80:30080/TCP, 53/UDP"),
        ]
    );
}

/// A LoadBalancer the cloud hasn't given an address yet reads as pending; a
/// ClusterIP Service has no external IP at all.
#[test]
fn a_service_without_an_external_address_says_why() {
    let pending = cells(
        &SERVICE,
        json!({
            "apiVersion": "v1", "kind": "Service",
            "metadata": { "name": "web", "namespace": "staging" },
            "spec": { "type": "LoadBalancer", "clusterIP": "10.96.0.10" },
        }),
    );
    let internal = cells(
        &SERVICE,
        json!({
            "apiVersion": "v1", "kind": "Service",
            "metadata": { "name": "db", "namespace": "staging" },
            "spec": { "clusterIP": "10.96.0.11" },
        }),
    );
    assert_eq!(pending[2], Cell::text("<pending>"));
    assert_eq!(
        internal[0],
        Cell::text("ClusterIP"),
        "the API's default type"
    );
    assert_eq!(internal[2], Cell::Empty);
}

#[test]
fn an_ingress_shows_class_hosts_address_and_ports() {
    let cells = cells(
        &INGRESS,
        json!({
            "apiVersion": "networking.k8s.io/v1", "kind": "Ingress",
            "metadata": { "name": "web", "namespace": "staging" },
            "spec": {
                "ingressClassName": "nginx",
                "rules": [{ "host": "app.example.com" }, { "host": "api.example.com" }],
                "tls": [{ "hosts": ["app.example.com"], "secretName": "app-tls" }],
            },
            "status": { "loadBalancer": { "ingress": [{ "hostname": "lb.example.com" }] } },
        }),
    );
    assert_eq!(
        cells,
        vec![
            Cell::text("nginx"),
            Cell::text("app.example.com, api.example.com"),
            Cell::text("lb.example.com"),
            Cell::text("80, 443"),
        ]
    );
}

/// Endpoints list each ready `ip:port`, the first three and then a count of
/// the rest, as `kubectl` does - not-ready addresses aren't serving.
#[test]
fn endpoints_list_ready_addresses_and_truncate_the_rest() {
    let cells = cells(
        &ENDPOINTS,
        json!({
            "apiVersion": "v1", "kind": "Endpoints",
            "metadata": { "name": "web", "namespace": "staging" },
            "subsets": [{
                "addresses": [
                    { "ip": "10.0.0.1" }, { "ip": "10.0.0.2" },
                    { "ip": "10.0.0.3" }, { "ip": "10.0.0.4" }, { "ip": "10.0.0.5" },
                ],
                "notReadyAddresses": [{ "ip": "10.0.0.9" }],
                "ports": [{ "port": 8080 }],
            }],
        }),
    );
    assert_eq!(
        cells,
        vec![Cell::text(
            "10.0.0.1:8080, 10.0.0.2:8080, 10.0.0.3:8080 + 2 more"
        )]
    );
}

#[test]
fn an_endpoint_slice_shows_address_type_ports_and_endpoints() {
    let cells = cells(
        &ENDPOINT_SLICE,
        json!({
            "apiVersion": "discovery.k8s.io/v1", "kind": "EndpointSlice",
            "metadata": { "name": "web-abc12", "namespace": "staging" },
            "addressType": "IPv4",
            "endpoints": [{ "addresses": ["10.0.0.1"] }, { "addresses": ["10.0.0.2"] }],
            "ports": [{ "port": 8080 }, { "port": 8443 }],
        }),
    );
    assert_eq!(
        cells,
        vec![
            Cell::text("IPv4"),
            Cell::text("8080, 8443"),
            Cell::text("10.0.0.1, 10.0.0.2"),
        ]
    );
}

/// A NetworkPolicy shows its pod selector; an empty one selects every pod in
/// the namespace and reads as `<none>`, as `kubectl` prints it.
#[test]
fn a_network_policy_shows_its_pod_selector() {
    let selected = cells(
        &NETWORK_POLICY,
        json!({
            "apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
            "metadata": { "name": "web", "namespace": "staging" },
            "spec": { "podSelector": {
                "matchLabels": { "app": "web" },
                "matchExpressions": [{ "key": "tier", "operator": "In", "values": ["a", "b"] }],
            } },
        }),
    );
    let everything = cells(
        &NETWORK_POLICY,
        json!({
            "apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
            "metadata": { "name": "deny-all", "namespace": "staging" },
            "spec": { "podSelector": {} },
        }),
    );
    assert_eq!(selected, vec![Cell::text("app=web, tier In (a, b)")]);
    assert_eq!(everything, vec![Cell::text("<none>")]);
}

/// A malformed Service gets every column empty, never a panic.
#[test]
fn a_malformed_service_gets_empty_cells() {
    let cells = cells(
        &SERVICE,
        json!({
            "apiVersion": "v1", "kind": "Service",
            "metadata": { "name": "web", "namespace": "staging" },
            "spec": { "ports": "eighty" },
        }),
    );
    assert_eq!(cells, vec![Cell::Empty; 4]);
}
