//! Network kinds' sections (`standard-resource-panels` 3.1): Service,
//! Ingress, Endpoints, EndpointSlice and NetworkPolicy.

use super::fixtures::{kind, object};
use super::sections::field;
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::object_detail::model::FieldValue;
use crate::k8s::resource::object_detail::sections::sections_for;
use serde_json::json;

#[test]
fn a_service_shows_type_ips_ports_selector_and_affinity() {
    let service = object(json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": { "name": "web", "namespace": "staging" },
        "spec": {
            "type": "LoadBalancer",
            "clusterIP": "10.96.0.10",
            "clusterIPs": ["10.96.0.10", "fd00::10"],
            "externalIPs": ["203.0.113.7"],
            "ports": [
                { "name": "http", "port": 80, "protocol": "TCP", "targetPort": 8080, "nodePort": 30080 },
                { "port": 443, "targetPort": "https" },
            ],
            "selector": { "app": "web" },
            "sessionAffinity": "ClientIP",
        },
        "status": { "loadBalancer": { "ingress": [{ "ip": "198.51.100.4" }, { "hostname": "lb.example.com" }] } },
    }));

    let sections = sections_for(&kind("", "v1", "Service", true), &service);

    assert_eq!(sections[0].title, "Service");
    assert_eq!(field(&sections, "Type").value.text(), "LoadBalancer");
    assert_eq!(
        field(&sections, "Cluster IPs").value.text(),
        "10.96.0.10, fd00::10"
    );
    assert_eq!(field(&sections, "External IPs").value.text(), "203.0.113.7");
    assert_eq!(
        field(&sections, "Load Balancer").value.text(),
        "198.51.100.4, lb.example.com"
    );
    assert_eq!(
        field(&sections, "Ports").value,
        FieldValue::Lines(vec![
            "http 80/TCP → 8080, node 30080".into(),
            "443/TCP → https".into(),
        ]),
        "each port with its protocol, target port and node port"
    );
    assert_eq!(field(&sections, "Selector").value.text(), "app=web");
    assert_eq!(
        field(&sections, "Session Affinity").value.text(),
        "ClientIP"
    );
}

#[test]
fn an_ingress_shows_rules_and_tls_and_references_backends_and_secrets() {
    let ingress = object(json!({
        "apiVersion": "networking.k8s.io/v1",
        "kind": "Ingress",
        "metadata": { "name": "web", "namespace": "staging" },
        "spec": {
            "ingressClassName": "nginx",
            "defaultBackend": { "service": { "name": "fallback", "port": { "number": 80 } } },
            "rules": [{
                "host": "app.example.com",
                "http": { "paths": [
                    { "path": "/", "pathType": "Prefix",
                      "backend": { "service": { "name": "web", "port": { "number": 80 } } } },
                    { "path": "/api", "pathType": "Prefix",
                      "backend": { "service": { "name": "api", "port": { "name": "http" } } } },
                    { "path": "/static", "pathType": "Prefix",
                      "backend": { "service": { "name": "web", "port": { "number": 80 } } } },
                ] },
            }],
            "tls": [{ "hosts": ["app.example.com"], "secretName": "app-tls" }],
        },
    }));

    let sections = sections_for(&kind("networking.k8s.io", "v1", "Ingress", true), &ingress);

    assert_eq!(field(&sections, "Class").value.text(), "nginx");
    assert_eq!(
        field(&sections, "Default Backend").value.text(),
        "fallback:80"
    );
    assert_eq!(
        field(&sections, "Rules").value,
        FieldValue::Lines(vec![
            "app.example.com/ → web:80".into(),
            "app.example.com/api → api:http".into(),
            "app.example.com/static → web:80".into(),
        ])
    );
    assert_eq!(
        field(&sections, "Backends").value,
        FieldValue::References {
            targets: vec![
                ObjectRef::core("Service", "staging", "fallback"),
                ObjectRef::core("Service", "staging", "web"),
                ObjectRef::core("Service", "staging", "api"),
            ],
            qualified: false,
        },
        "each backend Service once, the default backend's included"
    );
    assert_eq!(
        field(&sections, "Open").value,
        FieldValue::Urls(vec![
            "https://app.example.com/".into(),
            "https://app.example.com/api".into(),
            "https://app.example.com/static".into(),
        ]),
        "a TLS host's paths open over https"
    );
    assert_eq!(
        field(&sections, "TLS Hosts").value.text(),
        "app.example.com"
    );
    assert_eq!(
        field(&sections, "TLS Secrets").value,
        FieldValue::References {
            targets: vec![ObjectRef::core("Secret", "staging", "app-tls")],
            qualified: false,
        }
    );
}

#[test]
fn endpoints_show_addresses_with_ready_state_and_reference_target_pods() {
    let endpoints = object(json!({
        "apiVersion": "v1",
        "kind": "Endpoints",
        "metadata": { "name": "web", "namespace": "staging" },
        "subsets": [{
            "addresses": [
                { "ip": "10.244.1.5", "targetRef": { "kind": "Pod", "name": "web-1", "namespace": "staging" } },
            ],
            "notReadyAddresses": [
                { "ip": "10.244.2.9", "targetRef": { "kind": "Pod", "name": "web-2" } },
            ],
            "ports": [{ "name": "http", "port": 8080, "protocol": "TCP" }],
        }],
    }));

    let sections = sections_for(&kind("", "v1", "Endpoints", true), &endpoints);

    assert_eq!(
        field(&sections, "Addresses").value,
        FieldValue::Lines(vec![
            "10.244.1.5 (ready)".into(),
            "10.244.2.9 (not ready)".into(),
        ])
    );
    assert_eq!(field(&sections, "Ports").value.text(), "http 8080/TCP");
    assert_eq!(
        field(&sections, "Targets").value,
        FieldValue::References {
            targets: vec![
                ObjectRef::core("Pod", "staging", "web-1"),
                ObjectRef::core("Pod", "staging", "web-2"),
            ],
            qualified: true,
        },
        "a target with no namespace of its own is in the Endpoints' namespace"
    );
}

#[test]
fn an_endpoint_slice_shows_endpoints_and_ports_and_references_target_pods() {
    let slice = object(json!({
        "apiVersion": "discovery.k8s.io/v1",
        "kind": "EndpointSlice",
        "metadata": { "name": "web-abc12", "namespace": "staging" },
        "addressType": "IPv4",
        "endpoints": [
            { "addresses": ["10.244.1.5"], "conditions": { "ready": true },
              "targetRef": { "kind": "Pod", "name": "web-1", "namespace": "staging" } },
            { "addresses": ["10.244.2.9"], "conditions": { "ready": false },
              "targetRef": { "kind": "Pod", "name": "web-2", "namespace": "staging" } },
        ],
        "ports": [{ "name": "http", "port": 8080, "protocol": "TCP" }],
    }));

    let sections = sections_for(
        &kind("discovery.k8s.io", "v1", "EndpointSlice", true),
        &slice,
    );

    assert_eq!(field(&sections, "Address Type").value.text(), "IPv4");
    assert_eq!(
        field(&sections, "Endpoints").value,
        FieldValue::Lines(vec![
            "10.244.1.5 (ready)".into(),
            "10.244.2.9 (not ready)".into(),
        ])
    );
    assert_eq!(field(&sections, "Ports").value.text(), "http 8080/TCP");
    assert_eq!(
        field(&sections, "Targets").value.text(),
        "Pod/web-1, Pod/web-2"
    );
}

#[test]
fn a_network_policy_shows_its_selector_types_and_rules() {
    let policy = object(json!({
        "apiVersion": "networking.k8s.io/v1",
        "kind": "NetworkPolicy",
        "metadata": { "name": "web", "namespace": "staging" },
        "spec": {
            "podSelector": { "matchLabels": { "app": "web" } },
            "policyTypes": ["Ingress", "Egress"],
            "ingress": [{
                "from": [
                    { "podSelector": { "matchLabels": { "role": "frontend" } } },
                    { "namespaceSelector": { "matchLabels": { "team": "ops" } },
                      "podSelector": {} },
                    { "ipBlock": { "cidr": "10.0.0.0/8", "except": ["10.1.0.0/16"] } },
                ],
                "ports": [{ "protocol": "TCP", "port": 8080 }],
            }],
            "egress": [{ "ports": [{ "protocol": "UDP", "port": 53 }] }],
        },
    }));

    let sections = sections_for(
        &kind("networking.k8s.io", "v1", "NetworkPolicy", true),
        &policy,
    );

    assert_eq!(field(&sections, "Pod Selector").value.text(), "app=web");
    assert_eq!(
        field(&sections, "Policy Types").value.text(),
        "Ingress, Egress"
    );
    assert_eq!(
        field(&sections, "Ingress Rules").value,
        FieldValue::Lines(vec![
            "from pods role=frontend | all pods in namespaces team=ops | \
             ipBlock 10.0.0.0/8 except 10.1.0.0/16; ports 8080/TCP"
                .into(),
        ])
    );
    assert_eq!(
        field(&sections, "Egress Rules").value,
        FieldValue::Lines(vec!["to anywhere; ports 53/UDP".into()]),
        "a rule with no peers leaves every destination open"
    );
}

/// An empty pod selector selects every pod in the namespace - shown as that,
/// not as an empty row.
#[test]
fn an_empty_pod_selector_reads_as_every_pod() {
    let policy = object(json!({
        "apiVersion": "networking.k8s.io/v1",
        "kind": "NetworkPolicy",
        "metadata": { "name": "deny-all", "namespace": "staging" },
        "spec": { "podSelector": {}, "policyTypes": ["Ingress"] },
    }));

    let sections = sections_for(
        &kind("networking.k8s.io", "v1", "NetworkPolicy", true),
        &policy,
    );

    assert_eq!(
        field(&sections, "Pod Selector").value.text(),
        "all pods in the namespace"
    );
}

/// #157: a host without TLS opens over http, a rule with no paths at its root,
/// and a wildcard host - no one address - not at all.
#[test]
fn an_ingresss_hosts_open_over_their_scheme_and_a_wildcard_does_not() {
    let ingress = object(json!({
        "apiVersion": "networking.k8s.io/v1",
        "kind": "Ingress",
        "metadata": { "name": "web", "namespace": "staging" },
        "spec": {
            "rules": [
                { "host": "plain.example.com", "http": { "paths": [
                    { "path": "/app", "pathType": "Prefix",
                      "backend": { "service": { "name": "web", "port": { "number": 80 } } } },
                ] } },
                { "host": "bare.example.com" },
                { "host": "*.example.com" },
                { "http": { "paths": [] } },
            ],
        },
    }));

    let sections = sections_for(&kind("networking.k8s.io", "v1", "Ingress", true), &ingress);

    assert_eq!(
        field(&sections, "Open").value,
        FieldValue::Urls(vec![
            "http://plain.example.com/app".into(),
            "http://bare.example.com/".into(),
        ])
    );
}
