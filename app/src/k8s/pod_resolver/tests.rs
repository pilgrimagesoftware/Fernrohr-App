//! Resolving a target's pods against a fake cluster - 0, 1 and 3 matches, an
//! all-namespaces selector, Pending pods left out, a workload's own selector -
//! the selector sent as the list's `labelSelector`, and the container rule.

use super::{container_for, resolve_pods};
use crate::config::exec_library::{Target, WorkloadKind};
use crate::k8s::test_cluster::FakeCluster;
use crate::k8s::test_recorder::Recorder;
use gpui_kit::TestAppContext;
use k8s_openapi::api::core::v1::Pod;
use serde_json::{Value, json};

fn pod(namespace: &str, name: &str, phase: &str, app: &str) -> Value {
    json!({ "apiVersion": "v1", "kind": "Pod",
        "metadata": { "name": name, "namespace": namespace, "uid": format!("{namespace}-{name}"),
            "labels": { "app": app } },
        "spec": { "containers": [{ "name": "sidecar" }, { "name": "app" }] },
        "status": { "phase": phase } })
}

fn selector(namespace: Option<&str>, selector: &str) -> Target {
    Target::Selector {
        namespace: namespace.map(str::to_string),
        selector: selector.into(),
    }
}

fn names(pods: &[Pod]) -> Vec<String> {
    pods.iter()
        .map(|pod| {
            format!(
                "{}/{}",
                pod.metadata.namespace.as_deref().unwrap_or_default(),
                pod.metadata.name.as_deref().unwrap_or_default()
            )
        })
        .collect()
}

/// A fake cluster holding `web` pods in two namespaces (one Pending), an `api`
/// pod, and a Deployment selecting `app=web`.
fn cluster(cx: &mut TestAppContext) -> (kube::Client, tokio::runtime::Handle) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (cluster, client) = FakeCluster::start(cx);
    for (namespace, name, phase, app) in [
        ("shop", "web-2", "Running", "web"),
        ("shop", "web-1", "Running", "web"),
        ("shop", "web-0", "Pending", "web"),
        ("staging", "web-9", "Running", "web"),
        ("shop", "api-1", "Running", "api"),
    ] {
        cluster.apply("/api/v1", "pods", pod(namespace, name, phase, app));
    }
    cluster.apply(
        "/apis/apps/v1",
        "deployments",
        json!({ "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": { "name": "web", "namespace": "shop", "uid": "deploy-web" },
            "spec": { "selector": { "matchLabels": { "app": "web" } } } }),
    );
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    (client, handle)
}

#[gpui_kit::test]
async fn a_selector_resolves_its_running_pods_in_order(cx: &mut TestAppContext) {
    let (client, handle) = cluster(cx);
    let resolve = |target: Target| handle.block_on(resolve_pods(client.clone(), &target));

    assert_eq!(
        names(&resolve(selector(Some("shop"), "app=web")).unwrap()),
        ["shop/web-1", "shop/web-2"],
        "Running only, by name; the Pending web-0 left out"
    );
    assert_eq!(
        names(&resolve(selector(None, "app=web")).unwrap()),
        ["shop/web-1", "shop/web-2", "staging/web-9"],
        "every namespace, sorted by namespace then name"
    );
    assert_eq!(
        names(&resolve(selector(Some("shop"), "app=api")).unwrap()),
        ["shop/api-1"],
        "exactly one"
    );
    assert!(
        resolve(selector(Some("shop"), "app=db"))
            .unwrap()
            .is_empty(),
        "none"
    );
}

#[gpui_kit::test]
async fn a_workload_resolves_the_pods_its_own_selector_matches(cx: &mut TestAppContext) {
    let (client, handle) = cluster(cx);
    let deployment = Target::Workload {
        namespace: "shop".into(),
        workload_kind: WorkloadKind::Deployment,
        name: "web".into(),
    };
    let pods = handle
        .block_on(resolve_pods(client.clone(), &deployment))
        .unwrap();
    assert_eq!(names(&pods), ["shop/web-1", "shop/web-2"]);

    let missing = Target::Workload {
        namespace: "shop".into(),
        workload_kind: WorkloadKind::StatefulSet,
        name: "db".into(),
    };
    assert!(handle.block_on(resolve_pods(client, &missing)).is_err());
}

/// The selector is sent as the list's `labelSelector`, so a big cluster
/// filters on the server.
#[test]
fn the_selector_is_sent_to_the_server() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let (server, client) = Recorder::start(
        runtime.handle(),
        "200 OK",
        json!({ "apiVersion": "v1", "kind": "PodList", "metadata": {}, "items": [] }),
    );
    runtime
        .block_on(resolve_pods(client, &selector(None, "app=web,tier!=cache")))
        .unwrap();
    let target = &server.requests()[0].target;
    assert!(target.starts_with("/api/v1/pods?"), "{target}");
    assert!(
        target.contains("labelSelector=app%3Dweb%2Ctier%21%3Dcache"),
        "{target}"
    );
}

fn annotated(annotation: Option<&str>) -> Pod {
    let mut pod: Pod = serde_json::from_value(pod("shop", "web-1", "Running", "web")).unwrap();
    if let Some(annotation) = annotation {
        pod.metadata.annotations = Some(
            [(
                super::DEFAULT_CONTAINER_ANNOTATION.to_string(),
                annotation.to_string(),
            )]
            .into_iter()
            .collect(),
        );
    }
    pod
}

#[test]
fn the_container_is_the_entrys_then_the_default_annotation_then_the_first() {
    assert_eq!(container_for(&annotated(None), Some("app")).unwrap(), "app");
    assert_eq!(
        container_for(&annotated(Some("app")), None).unwrap(),
        "app",
        "spec: the annotation over the first container"
    );
    assert_eq!(container_for(&annotated(None), None).unwrap(), "sidecar");
    assert_eq!(
        container_for(&annotated(Some("gone")), None).unwrap(),
        "sidecar",
        "an annotation naming no container is ignored"
    );
    let missing = container_for(&annotated(None), Some("tools")).unwrap_err();
    assert!(
        missing.contains("tools") && missing.contains("shop/web-1"),
        "names the container and the pod: {missing}"
    );
}
