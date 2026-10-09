use crate::consts::{MCP_LOG_DEFAULT_TAIL_LINES, MCP_LOG_READ_CAP};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::mock_api::MockApi;
use crate::k8s::test_recorder::Recorder;
use crate::mcp::error::ToolError;
use crate::mcp::read::test_support::{call, open, tools};
use crate::mcp::tools::ToolContext;
use gpui_kit::TestAppContext;
use serde_json::{Value, json};

const LOG_PATH: &str = "/api/v1/namespaces/team-a/pods/api/log";

/// A connected `dev` context whose cluster serves `log` as pod `api`'s logs.
fn serving(cx: &mut TestAppContext, log: &str) -> ToolContext {
    let tools = tools(cx, &["dev"]);
    let api = MockApi::start(&[(LOG_PATH, 200, log)]);
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let client = {
        let _runtime = handle.enter();
        api.client()
    };
    open(cx, "dev", ConnectionState::Connected(client), None);
    tools
}

fn logs_of(pod: &str) -> Value {
    json!({"context": "dev", "namespace": "team-a", "pod": pod})
}

#[gpui_kit::test]
async fn a_pods_logs_are_returned(cx: &mut TestAppContext) {
    let tools = serving(cx, "starting\nlistening on :8080\n");
    let output = call(cx, &tools, "get_pod_logs", logs_of("api"))
        .await
        .unwrap();
    assert_eq!(output.content["logs"], "starting\nlistening on :8080\n");
    assert_eq!(output.content["pod"], "api");
    assert!(!output.truncated);
}

#[gpui_kit::test]
async fn oversized_logs_keep_the_newest_lines_within_the_limit(cx: &mut TestAppContext) {
    let log: String = (0..3000)
        .map(|n| format!("line {n:05} {}\n", "x".repeat(80)))
        .collect();
    let tools = serving(cx, &log);
    let mut arguments = logs_of("api");
    arguments["limit_bytes"] = json!(1000);
    let output = call(cx, &tools, "get_pod_logs", arguments).await.unwrap();

    let logs = output.content["logs"].as_str().unwrap();
    assert!(output.truncated);
    assert!(logs.len() <= 1000, "{} bytes", logs.len());
    assert!(logs.starts_with("line "), "starts mid-line: {logs:.20}");
    assert!(logs.ends_with(&format!("line 02999 {}\n", "x".repeat(80))));
}

#[gpui_kit::test]
async fn the_request_is_bounded_and_carries_the_container(cx: &mut TestAppContext) {
    let tools = tools(cx, &["dev"]);
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let (recorder, client) = Recorder::start(&handle, "200 OK", json!("log"));
    open(cx, "dev", ConnectionState::Connected(client), None);
    let mut arguments = logs_of("api");
    arguments["container"] = json!("sidecar");
    arguments["previous"] = json!(true);
    call(cx, &tools, "get_pod_logs", arguments).await.unwrap();

    let requests = recorder.requests();
    assert_eq!(requests.len(), 1);
    let target = &requests[0].target;
    assert!(target.starts_with(&format!("{LOG_PATH}?")), "{target}");
    for part in [
        "container=sidecar".to_string(),
        "previous=true".to_string(),
        format!("tailLines={MCP_LOG_DEFAULT_TAIL_LINES}"),
        format!("limitBytes={MCP_LOG_READ_CAP}"),
    ] {
        assert!(target.contains(&part), "{target} lacks {part}");
    }
    assert!(!target.contains("follow=true"), "{target}");
}

#[gpui_kit::test]
async fn bad_arguments_never_reach_the_cluster(cx: &mut TestAppContext) {
    let tools = tools(cx, &["dev"]);
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let (recorder, client) = Recorder::start(&handle, "200 OK", json!("log"));
    open(cx, "dev", ConnectionState::Connected(client), None);
    let with = |field: &str, value: Value| {
        let mut arguments = logs_of("api");
        arguments[field] = value;
        arguments
    };
    for arguments in [
        logs_of("api/exec"),
        with("namespace", json!("../kube-system")),
        with("container", json!("Side Car")),
        with("tail_lines", json!(0)),
        with("tail_lines", json!(5001)),
        with("limit_bytes", json!(0)),
        with("limit_bytes", json!(2 * 1024 * 1024)),
    ] {
        let result = call(cx, &tools, "get_pod_logs", arguments.clone()).await;
        assert!(
            matches!(result, Err(ToolError::InvalidArguments { .. })),
            "{arguments}: {result:?}"
        );
    }
    assert!(recorder.requests().is_empty());
}

#[gpui_kit::test]
async fn a_missing_pod_is_a_kubernetes_not_found(cx: &mut TestAppContext) {
    let tools = tools(cx, &["dev"]);
    let not_found = json!({"kind": "Status", "apiVersion": "v1", "status": "Failure",
        "reason": "NotFound", "code": 404, "message": "pods \"gone\" not found"});
    let api = MockApi::start(&[(
        "/api/v1/namespaces/team-a/pods/gone/log",
        404,
        &not_found.to_string(),
    )]);
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let client = {
        let _runtime = handle.enter();
        api.client()
    };
    open(cx, "dev", ConnectionState::Connected(client), None);
    let result = call(cx, &tools, "get_pod_logs", logs_of("gone")).await;
    assert!(
        matches!(result, Err(ToolError::Kubernetes { status: 404, .. })),
        "{result:?}"
    );
}
