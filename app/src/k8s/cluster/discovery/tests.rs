use super::*;
use kube::{Client, Config};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

/// Serves fixed JSON responses by request path until the listener (and
/// every clone of it) is dropped, i.e. for the lifetime of the test.
async fn serve_fixtures(routes: HashMap<&'static str, &'static str>) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let routes = Arc::new(routes);
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let routes = routes.clone();
            tokio::spawn(handle_connection(stream, routes));
        }
    });
    addr
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    routes: Arc<HashMap<&'static str, &'static str>>,
) {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).await.unwrap_or(0) == 0 {
        return;
    }
    // Drain headers up to the blank line.
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await.unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
    }

    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .to_string();
    let stream = reader.into_inner();
    respond(stream, routes.get(path.as_str()).copied()).await;
}

async fn respond(mut stream: tokio::net::TcpStream, body: Option<&str>) {
    let (status, body) = match body {
        Some(body) => ("200 OK", body),
        None => ("404 Not Found", "{}"),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

fn client_for(addr: std::net::SocketAddr) -> Client {
    let mut config = Config::new(format!("http://{addr}").parse().unwrap());
    config.connect_timeout = Some(std::time::Duration::from_millis(500));
    Client::try_from(config).unwrap()
}

#[tokio::test]
async fn pod_is_present_after_discovery() {
    let routes = HashMap::from([
        (
            "/apis",
            r#"{"kind":"APIGroupList","apiVersion":"v1","groups":[]}"#,
        ),
        (
            "/api",
            r#"{"kind":"APIVersions","versions":["v1"],"serverAddressByClientCIDRs":[]}"#,
        ),
        (
            "/api/v1",
            r#"{"kind":"APIResourceList","groupVersion":"v1","resources":[
                {"name":"pods","singularName":"pod","namespaced":true,"kind":"Pod","verbs":["delete","get","list","watch"]}
            ]}"#,
        ),
    ]);
    let addr = serve_fixtures(routes).await;

    let kinds = discover_kinds(client_for(addr)).await.unwrap().kinds;

    let pod = kinds
        .iter()
        .find(|kind| kind.gvk.kind == "Pod")
        .expect("Pod is discovered");
    assert_eq!(pod.gvk.group, "", "core group has an empty group name");
    assert_eq!(pod.gvk.version, "v1");
    assert_eq!(pod.plural, "pods");
    assert!(pod.namespaced, "Pods are namespaced");
    assert_eq!(pod.verbs, KindVerbs::default(), "Pods list and watch");
}

/// A cluster serving a CRD alongside its built-in resources: the CRD's kind
/// comes back from discovery like any other, which is what lets the
/// Resource panel list it instead of a fixed subset of built-ins.
#[tokio::test]
async fn a_crd_kind_is_discovered_alongside_the_built_in_resources() {
    let routes = HashMap::from([
        (
            "/apis",
            r#"{"kind":"APIGroupList","apiVersion":"v1","groups":[{
                "name":"ferns.example.com",
                "versions":[{"groupVersion":"ferns.example.com/v1","version":"v1"}],
                "preferredVersion":{"groupVersion":"ferns.example.com/v1","version":"v1"}
            }]}"#,
        ),
        (
            "/api",
            r#"{"kind":"APIVersions","versions":["v1"],"serverAddressByClientCIDRs":[]}"#,
        ),
        (
            "/api/v1",
            r#"{"kind":"APIResourceList","groupVersion":"v1","resources":[
                {"name":"pods","singularName":"pod","namespaced":true,"kind":"Pod","verbs":["get","list","watch"]}
            ]}"#,
        ),
        (
            "/apis/ferns.example.com/v1",
            r#"{"kind":"APIResourceList","apiVersion":"v1","groupVersion":"ferns.example.com/v1","resources":[
                {"name":"ferns","singularName":"fern","namespaced":true,"kind":"Fern","verbs":["get","list","watch"]}
            ]}"#,
        ),
    ]);
    let addr = serve_fixtures(routes).await;

    let kinds = discover_kinds(client_for(addr)).await.unwrap().kinds;

    let fern = kinds
        .iter()
        .find(|kind| kind.gvk.kind == "Fern")
        .expect("the CRD's kind is discovered");
    assert_eq!(fern.gvk.group, "ferns.example.com");
    assert_eq!(fern.gvk.version, "v1");
    assert_eq!(fern.plural, "ferns");
    assert!(fern.namespaced);

    // The built-in resources are still there - the CRD is additive, not a
    // replacement for the core group.
    assert!(
        kinds.iter().any(|kind| kind.gvk.kind == "Pod"),
        "kinds: {kinds:?}"
    );
}

/// Same-named kinds under different groups stay separate rows, and each is
/// labelled so a reader can tell which is which - the reason a discovered
/// kind carries its group rather than just its name.
#[tokio::test]
async fn same_named_kinds_in_different_groups_stay_distinct() {
    let routes = HashMap::from([
        (
            "/apis",
            r#"{"kind":"APIGroupList","apiVersion":"v1","groups":[{
                "name":"events.k8s.io",
                "versions":[{"groupVersion":"events.k8s.io/v1","version":"v1"}],
                "preferredVersion":{"groupVersion":"events.k8s.io/v1","version":"v1"}
            }]}"#,
        ),
        (
            "/api",
            r#"{"kind":"APIVersions","versions":["v1"],"serverAddressByClientCIDRs":[]}"#,
        ),
        (
            "/api/v1",
            r#"{"kind":"APIResourceList","groupVersion":"v1","resources":[
                {"name":"events","singularName":"event","namespaced":true,"kind":"Event","verbs":["get","list","watch"]}
            ]}"#,
        ),
        (
            "/apis/events.k8s.io/v1",
            r#"{"kind":"APIResourceList","apiVersion":"v1","groupVersion":"events.k8s.io/v1","resources":[
                {"name":"events","singularName":"event","namespaced":true,"kind":"Event","verbs":["get","list","watch"]}
            ]}"#,
        ),
    ]);
    let addr = serve_fixtures(routes).await;

    let kinds = discover_kinds(client_for(addr)).await.unwrap().kinds;

    let events: Vec<&DiscoveredKind> = kinds
        .iter()
        .filter(|kind| kind.gvk.kind == "Event")
        .collect();
    assert_eq!(events.len(), 2, "one row per group, not one per name");
    assert_eq!(events[0].label(), "Event", "core needs no qualifier");
    assert_eq!(events[1].label(), "Event · events.k8s.io");
}

/// A resource that only supports `get` is still a discovered kind, so it
/// still gets a row - the Resource panel lists what the cluster reports
/// rather than filtering down to what it can render today.
#[tokio::test]
async fn a_kind_without_list_verb_is_still_listed() {
    let routes = HashMap::from([
        (
            "/apis",
            r#"{"kind":"APIGroupList","apiVersion":"v1","groups":[]}"#,
        ),
        (
            "/api",
            r#"{"kind":"APIVersions","versions":["v1"],"serverAddressByClientCIDRs":[]}"#,
        ),
        (
            "/api/v1",
            r#"{"kind":"APIResourceList","groupVersion":"v1","resources":[
                {"name":"componentstatuses","singularName":"","namespaced":false,"kind":"ComponentStatus","verbs":["get","list"]},
                {"name":"bindings","singularName":"","namespaced":true,"kind":"Binding","verbs":["create"]},
                {"name":"secrets","singularName":"secret","namespaced":true,"kind":"Secret","verbs":["delete","get","list","watch"]}
            ]}"#,
        ),
    ]);
    let addr = serve_fixtures(routes).await;

    let kinds = discover_kinds(client_for(addr)).await.unwrap().kinds;

    let status = kinds
        .iter()
        .find(|kind| kind.gvk.kind == "ComponentStatus")
        .expect("discovered regardless of its verbs");
    assert!(!status.namespaced, "ComponentStatus is cluster-scoped");
    // `unwatchable-kinds`: its verbs come along, so its list panel polls.
    assert_eq!(
        status.verbs,
        KindVerbs {
            list: true,
            watch: false,
            delete: false
        }
    );
    let binding = kinds
        .iter()
        .find(|kind| kind.gvk.kind == "Binding")
        .expect("discovered regardless of its verbs");
    assert!(!binding.verbs.list, "a create-only kind can't be listed");
    assert!(!binding.verbs.delete, "nor deleted");
    let secret = kinds
        .iter()
        .find(|kind| kind.gvk.kind == "Secret")
        .expect("discovered");
    assert!(secret.verbs.delete, "a kind discovery lists `delete` for");
}

#[test]
fn kinds_sort_by_group_then_kind_with_core_first() {
    let kinds = vec![
        DiscoveredKind {
            gvk: GroupVersionKind::gvk("apps", "v1", "Deployment"),
            plural: "deployments".into(),
            namespaced: true,
            verbs: Default::default(),
        },
        DiscoveredKind {
            gvk: GroupVersionKind::gvk("", "v1", "Service"),
            plural: "services".into(),
            namespaced: true,
            verbs: Default::default(),
        },
        DiscoveredKind {
            gvk: GroupVersionKind::gvk("", "v1", "Pod"),
            plural: "pods".into(),
            namespaced: true,
            verbs: Default::default(),
        },
    ];
    let mut sorted = kinds.clone();
    sorted.sort();

    let labels: Vec<String> = sorted.iter().map(DiscoveredKind::label).collect();
    assert_eq!(labels, vec!["Pod", "Service", "Deployment · apps"]);
}

#[test]
fn plural_label_capitalizes_the_plural_and_keeps_the_group_qualifier() {
    let pod = DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "Pod"),
        plural: "pods".into(),
        namespaced: true,
        verbs: Default::default(),
    };
    assert_eq!(pod.plural_label(), "Pods");

    let widget = DiscoveredKind {
        gvk: GroupVersionKind::gvk("example.com", "v1", "Widget"),
        plural: "widgets".into(),
        namespaced: true,
        verbs: Default::default(),
    };
    assert_eq!(widget.plural_label(), "Widgets · example.com");
    assert_eq!(widget.plural_name(), "Widgets");
}

use crate::k8s::cluster::mock_api::{cluster_with_a_failing_aggregated_group, recover_metrics};

/// One failing group costs only its own kinds: the core and `apps` kinds still
/// list, and the failure names the group with its HTTP status and message.
#[tokio::test]
async fn a_failing_group_is_reported_and_the_rest_still_list() {
    let api = cluster_with_a_failing_aggregated_group();

    let discovered = discover_kinds(api.client())
        .await
        .expect("listing groups works");

    let kinds: Vec<&str> = discovered
        .kinds
        .iter()
        .map(|kind| kind.gvk.kind.as_str())
        .collect();
    assert!(
        kinds.contains(&"Pod") && kinds.contains(&"Deployment"),
        "{kinds:?}"
    );
    assert!(!kinds.contains(&"PodMetrics"));
    assert_eq!(
        discovered.unavailable.len(),
        1,
        "{:?}",
        discovered.unavailable
    );
    let failed = &discovered.unavailable[0];
    assert_eq!(failed.group, "metrics.k8s.io");
    assert!(
        failed.reason.contains("503") && failed.reason.contains("service unavailable"),
        "readable, with the status: {}",
        failed.reason
    );
    assert!(
        !failed.reason.contains("Status {"),
        "not a Debug dump: {}",
        failed.reason
    );

    // Once the group recovers, the next discovery picks it up.
    recover_metrics(&api);
    let again = discover_kinds(api.client()).await.unwrap();
    assert!(again.unavailable.is_empty(), "{:?}", again.unavailable);
    assert!(again.kinds.iter().any(|kind| kind.gvk.kind == "PodMetrics"));
}

/// Listing the groups at all is the one failure that is fatal.
#[tokio::test]
async fn failing_to_list_groups_fails_discovery() {
    let api = crate::k8s::cluster::mock_api::cluster_whose_group_list_fails();
    assert!(discover_kinds(api.client()).await.is_err());
}

/// A group that keeps answering 503 is retried by kube's default client for far
/// longer than the panel should wait: past the per-group limit it's reported
/// unavailable, and the others' kinds still list.
#[tokio::test]
async fn a_group_retried_past_the_limit_is_reported_and_the_rest_list() {
    let api = cluster_with_a_failing_aggregated_group();
    let started = std::time::Instant::now();

    let discovered = discover_kinds_within(
        api.client_with_retries(true),
        std::time::Duration::from_millis(300),
    )
    .await
    .unwrap();

    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "bounded"
    );
    assert!(
        discovered
            .kinds
            .iter()
            .any(|kind| kind.gvk.kind == "Deployment")
    );
    assert_eq!(discovered.unavailable.len(), 1);
    assert_eq!(discovered.unavailable[0].group, "metrics.k8s.io");
    assert!(
        discovered.unavailable[0]
            .reason
            .contains("no answer within"),
        "{}",
        discovered.unavailable[0].reason
    );
}

/// Listing the groups, retried by kube's default client, is bounded the same way.
#[tokio::test]
async fn listing_groups_retried_past_the_limit_fails_promptly() {
    let api = crate::k8s::cluster::mock_api::cluster_whose_group_list_fails();
    let started = std::time::Instant::now();
    let failure = discover_kinds_within(
        api.client_with_retries(true),
        std::time::Duration::from_millis(300),
    )
    .await
    .expect_err("listing groups never answers in time");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert!(failure.message.contains("within"), "{}", failure.message);
}
