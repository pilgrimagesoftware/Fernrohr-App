use crate::consts::MCP_RESULT_BUDGET_BYTES;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::{DiscoveredKind, KindVerbs};
use crate::k8s::test_cluster::FakeCluster;
use crate::k8s::test_recorder::Recorder;
use crate::mcp::error::ToolError;
use crate::mcp::read::test_support::{call, open, tools};
use crate::mcp::test_support::discovered;
use crate::mcp::tools::ToolContext;
use gpui_kit::TestAppContext;
use serde_json::{Value, json};

fn kinds() -> Vec<DiscoveredKind> {
    vec![
        discovered("", "v1", "Pod", "pods", true),
        discovered("", "v1", "Secret", "secrets", true),
        discovered("", "v1", "Namespace", "namespaces", false),
        discovered("apps", "v1", "Deployment", "deployments", true),
    ]
}

fn pod(namespace: &str, name: &str) -> Value {
    json!({
        "apiVersion": "v1", "kind": "Pod",
        "metadata": {
            "name": name, "namespace": namespace,
            "managedFields": [{"manager": "kubectl", "operation": "Apply"}],
        },
        "spec": {"containers": [{"name": "app", "image": "nginx"}]},
    })
}

/// A connected `dev` context backed by a fake cluster holding three pods.
fn cluster(cx: &mut TestAppContext) -> (ToolContext, FakeCluster) {
    let tools = tools(cx, &["dev"]);
    let (cluster, client) = FakeCluster::start(cx);
    for (namespace, name) in [("team-a", "api"), ("team-a", "web"), ("team-b", "db")] {
        cluster.apply("/api/v1", "pods", pod(namespace, name));
    }
    open(cx, "dev", ConnectionState::Connected(client), Some(kinds()));
    (tools, cluster)
}

/// A connected `dev` context whose cluster records requests and answers
/// every one with `body`.
fn recorded(
    cx: &mut TestAppContext,
    body: Value,
    kinds: Vec<DiscoveredKind>,
) -> (ToolContext, Recorder) {
    let tools = tools(cx, &["dev"]);
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let (recorder, client) = Recorder::start(&handle, "200 OK", body);
    open(cx, "dev", ConnectionState::Connected(client), Some(kinds));
    (tools, recorder)
}

fn names(content: &serde_json::Map<String, Value>) -> Vec<String> {
    content["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["metadata"]["name"].as_str().unwrap().to_string())
        .collect()
}

#[gpui_kit::test]
async fn an_agent_reads_a_pod(cx: &mut TestAppContext) {
    let (tools, _cluster) = cluster(cx);
    let output = call(
        cx,
        &tools,
        "get_resource",
        json!({"context": "dev", "kind": "Pod", "namespace": "team-a", "name": "web"}),
    )
    .await
    .unwrap();
    let object = &output.content["object"];
    assert_eq!(object["metadata"]["name"], "web");
    assert_eq!(object["spec"]["containers"][0]["image"], "nginx");
    assert_eq!(object["apiVersion"], "v1");
    assert_eq!(object["kind"], "Pod");
    assert!(object["metadata"].get("managedFields").is_none());
}

#[gpui_kit::test]
async fn pods_are_listed_in_one_namespace_or_all(cx: &mut TestAppContext) {
    let (tools, _cluster) = cluster(cx);
    let output = call(
        cx,
        &tools,
        "list_resources",
        json!({"context": "dev", "kind": "pods", "namespace": "team-a"}),
    )
    .await
    .unwrap();
    assert_eq!(names(&output.content), ["api", "web"]);
    assert_eq!(output.content["count"], 2);
    let item = &output.content["items"][0];
    assert_eq!(item["kind"], "Pod");
    assert!(item["metadata"].get("managedFields").is_none());

    let output = call(
        cx,
        &tools,
        "list_resources",
        json!({"context": "dev", "kind": "pod"}),
    )
    .await
    .unwrap();
    assert_eq!(names(&output.content).len(), 3);
    assert_eq!(output.content["namespace"], Value::Null);
}

#[gpui_kit::test]
async fn secret_values_never_leave_the_app(cx: &mut TestAppContext) {
    let (tools, cluster) = cluster(cx);
    cluster.apply(
        "/api/v1",
        "secrets",
        json!({
            "apiVersion": "v1", "kind": "Secret",
            "metadata": {"name": "db", "namespace": "team-a"},
            "data": {"password": "aHVudGVyMg=="},
            "stringData": {"token": "plain-secret"},
        }),
    );
    let get = call(
        cx,
        &tools,
        "get_resource",
        json!({"context": "dev", "kind": "Secret", "namespace": "team-a", "name": "db"}),
    )
    .await
    .unwrap();
    let list = call(
        cx,
        &tools,
        "list_resources",
        json!({"context": "dev", "kind": "secrets"}),
    )
    .await
    .unwrap();
    for text in [Value::Object(get.content), Value::Object(list.content)].map(|v| v.to_string()) {
        assert!(!text.contains("aHVudGVyMg=="), "{text}");
        assert!(!text.contains("plain-secret"), "{text}");
        assert!(text.contains("<redacted: 7 bytes>"), "{text}");
    }
}

#[gpui_kit::test]
async fn a_missing_object_is_a_kubernetes_not_found(cx: &mut TestAppContext) {
    let (tools, _cluster) = cluster(cx);
    let result = call(
        cx,
        &tools,
        "get_resource",
        json!({"context": "dev", "kind": "Pod", "namespace": "team-a", "name": "gone"}),
    )
    .await;
    let Err(ToolError::Kubernetes { status, reason, .. }) = result else {
        panic!("expected a Kubernetes error, got {result:?}");
    };
    assert_eq!((status, reason.as_str()), (404, "NotFound"));
}

#[gpui_kit::test]
async fn bad_requests_never_reach_the_cluster(cx: &mut TestAppContext) {
    let (tools, recorder) = recorded(cx, json!({}), kinds());
    let get = |kind: &str, namespace: Option<&str>, name: &str| json!({"context": "dev", "kind": kind, "namespace": namespace, "name": name});
    let cases = [
        (
            "get_resource",
            get("CronJob", Some("team-a"), "x"),
            "unsupported_kind",
        ),
        (
            "get_resource",
            get("Pod", Some("team-a"), "../secrets/db"),
            "invalid_arguments",
        ),
        (
            "get_resource",
            get("Pod", Some("team-a"), "x?watch=true"),
            "invalid_arguments",
        ),
        (
            "get_resource",
            get("Pod", Some("Team A"), "x"),
            "invalid_arguments",
        ),
        ("get_resource", get("Pod", None, "x"), "invalid_arguments"),
        (
            "get_resource",
            get("Namespace", Some("team-a"), "x"),
            "invalid_arguments",
        ),
        (
            "list_resources",
            json!({"context": "dev", "kind": "Namespace", "namespace": "team-a"}),
            "invalid_arguments",
        ),
        (
            "list_resources",
            json!({"context": "dev", "kind": "Pod", "limit": 501}),
            "invalid_arguments",
        ),
        (
            "list_resources",
            json!({"context": "dev", "kind": "Pod", "group": "apps"}),
            "unsupported_kind",
        ),
    ];
    for (tool, arguments, code) in cases {
        let result = call(cx, &tools, tool, arguments.clone()).await;
        let error = result.expect_err(&arguments.to_string());
        assert_eq!(error.code(), code, "{arguments}");
    }
    assert!(recorder.requests().is_empty(), "{:?}", recorder.requests());
}

#[gpui_kit::test]
async fn a_kind_that_cant_be_listed_says_so(cx: &mut TestAppContext) {
    let mut only_get = discovered("metrics.k8s.io", "v1beta1", "PodMetrics", "pods", true);
    only_get.verbs = KindVerbs {
        list: false,
        ..KindVerbs::default()
    };
    let (tools, recorder) = recorded(cx, json!({}), vec![only_get]);
    let result = call(
        cx,
        &tools,
        "list_resources",
        json!({"context": "dev", "kind": "PodMetrics"}),
    )
    .await;
    assert_eq!(
        result,
        Err(ToolError::UnsupportedOperation {
            kind: "PodMetrics".into(),
            operation: "list".into()
        })
    );
    assert!(recorder.requests().is_empty());
}

#[gpui_kit::test]
async fn a_page_carries_its_selectors_and_continue_token(cx: &mut TestAppContext) {
    let page = json!({
        "apiVersion": "v1", "kind": "PodList",
        "metadata": {"continue": "next-page"},
        "items": [],
    });
    let (tools, recorder) = recorded(cx, page, kinds());
    let output = call(
        cx,
        &tools,
        "list_resources",
        json!({
            "context": "dev", "kind": "Pod", "namespace": "team-a",
            "label_selector": "app=web", "field_selector": "status.phase=Running",
            "limit": 5, "continue": "this-page",
        }),
    )
    .await
    .unwrap();
    assert_eq!(output.content["continue"], "next-page");

    let requests = recorder.requests();
    assert_eq!(requests.len(), 1);
    let target = &requests[0].target;
    assert!(
        target.starts_with("/api/v1/namespaces/team-a/pods?"),
        "{target}"
    );
    for part in [
        "limit=5",
        "continue=this-page",
        "labelSelector=app%3Dweb",
        "fieldSelector=status.phase%3DRunning",
    ] {
        assert!(target.contains(part), "{target} lacks {part}");
    }
}

/// A pod in `team-c` carrying an annotation of `pad` bytes.
fn padded_pod(name: &str, pad: usize) -> Value {
    let mut pod = pod("team-c", name);
    pod["metadata"]["annotations"] = json!({"pad": "x".repeat(pad)});
    pod
}

#[gpui_kit::test]
async fn an_oversized_page_is_cut_to_the_budget_and_says_so(cx: &mut TestAppContext) {
    let (tools, cluster) = cluster(cx);
    // 30 objects of ~50 KiB: half again the budget.
    for n in 0..30 {
        cluster.apply(
            "/api/v1",
            "pods",
            padded_pod(&format!("big-{n:02}"), 50 * 1024),
        );
    }
    let output = call(
        cx,
        &tools,
        "list_resources",
        json!({"context": "dev", "kind": "Pod", "namespace": "team-c"}),
    )
    .await
    .unwrap();

    assert!(output.truncated);
    assert_eq!(output.content["page_size"], 30);
    let kept = output.content["items"].as_array().unwrap();
    assert!(!kept.is_empty() && kept.len() < 30, "{} kept", kept.len());
    assert_eq!(output.content["count"], kept.len());
    let size = serde_json::to_vec(kept).unwrap().len();
    assert!(size <= MCP_RESULT_BUDGET_BYTES, "{size} bytes");
    assert_eq!(output.content["continue"], Value::Null);
}

#[gpui_kit::test]
async fn an_object_larger_than_the_budget_is_refused(cx: &mut TestAppContext) {
    let (tools, cluster) = cluster(cx);
    cluster.apply(
        "/api/v1",
        "pods",
        padded_pod("huge", MCP_RESULT_BUDGET_BYTES),
    );
    let result = call(
        cx,
        &tools,
        "get_resource",
        json!({"context": "dev", "kind": "Pod", "namespace": "team-c", "name": "huge"}),
    )
    .await;
    assert_eq!(
        result,
        Err(ToolError::ResultTooLarge {
            limit: MCP_RESULT_BUDGET_BYTES
        })
    );
}
