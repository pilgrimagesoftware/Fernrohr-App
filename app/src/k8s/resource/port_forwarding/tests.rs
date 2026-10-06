//! What a resource offers to forward, and where a Service's forward goes: its
//! selected, Running Pod and the port its `targetPort` names there.

use super::{matches_selector, pod_ports, resolve_service, service_ports};
use crate::k8s::test_cluster::FakeCluster;
use gpui_kit::TestAppContext;
use k8s_openapi::api::core::v1::{Pod, Service};
use serde_json::json;
use std::collections::BTreeMap;

fn web_pod(name: &str, phase: &str, app: &str) -> serde_json::Value {
    json!({ "apiVersion": "v1", "kind": "Pod",
        "metadata": { "name": name, "namespace": "shop", "uid": name, "labels": { "app": app } },
        "spec": { "containers": [
            { "name": "app", "image": "nginx", "ports": [{ "containerPort": 8080, "name": "http" }] },
            { "name": "sidecar", "image": "envoy", "ports": [
                { "containerPort": 9901, "name": "admin" },
                { "containerPort": 8080, "name": "dup" },
            ] },
        ] },
        "status": { "phase": phase } })
}

fn web_service() -> serde_json::Value {
    json!({ "apiVersion": "v1", "kind": "Service",
        "metadata": { "name": "web", "namespace": "shop", "uid": "svc" },
        "spec": {
            "selector": { "app": "web" },
            "ports": [
                { "name": "http", "port": 80, "targetPort": "http" },
                { "name": "admin", "port": 9000, "targetPort": 9901 },
            ],
        } })
}

#[test]
fn a_pods_ports_are_listed_once_each() {
    let pod: Pod = serde_json::from_value(web_pod("web-1", "Running", "web")).unwrap();
    let ports: Vec<(u16, String)> = pod_ports(&pod)
        .into_iter()
        .map(|choice| (choice.port, choice.label))
        .collect();
    assert_eq!(
        ports,
        [
            (8080, "8080 (http, app)".to_string()),
            (9901, "9901 (admin, sidecar)".to_string()),
        ]
    );
}

#[test]
fn a_services_ports_are_its_own_ports() {
    let service: Service = serde_json::from_value(web_service()).unwrap();
    let ports: Vec<u16> = service_ports(&service)
        .iter()
        .map(|choice| choice.port)
        .collect();
    assert_eq!(ports, [80, 9000]);
}

#[test]
fn a_selector_matches_only_labels_carrying_all_of_it() {
    let selector = BTreeMap::from([("app".to_string(), "web".to_string())]);
    let web = BTreeMap::from([
        ("app".to_string(), "web".to_string()),
        ("tier".to_string(), "front".to_string()),
    ]);
    let api = BTreeMap::from([("app".to_string(), "api".to_string())]);
    assert!(matches_selector(Some(&web), &selector));
    assert!(!matches_selector(Some(&api), &selector));
    assert!(!matches_selector(None, &selector));
    assert!(
        !matches_selector(Some(&web), &BTreeMap::new()),
        "no selector, no pods"
    );
}

/// A Service port forwards to a Running Pod it selects - not a Pending one, nor
/// another app's - on the port its `targetPort` names, by name or number.
#[gpui_kit::test]
async fn a_service_port_resolves_to_a_running_selected_pods_port(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply("/api/v1", "services", web_service());
    cluster.apply("/api/v1", "pods", web_pod("api-1", "Running", "api"));
    cluster.apply("/api/v1", "pods", web_pod("web-0", "Pending", "web"));
    cluster.apply("/api/v1", "pods", web_pod("web-1", "Running", "web"));
    let handle = cx.update(|cx| crate::runtime::handle(cx));

    let by_name = handle
        .block_on(resolve_service(client.clone(), "shop", "web", 80))
        .expect("resolved");
    assert_eq!(
        by_name,
        ("web-1".to_string(), 8080),
        "targetPort `http` by name"
    );

    let by_number = handle
        .block_on(resolve_service(client.clone(), "shop", "web", 9000))
        .expect("resolved");
    assert_eq!(
        by_number,
        ("web-1".to_string(), 9901),
        "targetPort 9901 by number"
    );

    let missing = handle.block_on(resolve_service(client, "shop", "web", 1234));
    assert!(missing.unwrap_err().contains("no port 1234"));
}
