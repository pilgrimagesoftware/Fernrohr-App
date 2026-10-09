use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery_registry::DiscoveryRegistry;
use crate::k8s::cluster::mock_api::cluster_with_a_failing_aggregated_group;
use crate::k8s::test_recorder::Recorder;
use crate::mcp::error::ToolError;
use crate::mcp::read::test_support::{call, open, tools};
use crate::mcp::test_support::discovered;
use gpui_kit::TestAppContext;
use serde_json::{Value, json};

fn kind_names(content: &serde_json::Map<String, Value>) -> Vec<String> {
    content["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|kind| kind["kind"].as_str().unwrap().to_string())
        .collect()
}

#[gpui_kit::test]
async fn contexts_are_listed_with_their_status(cx: &mut TestAppContext) {
    let tools = tools(cx, &["dev", "prod", "staging"]);
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let (_recorder, client) = Recorder::start(&handle, "200 OK", json!({}));
    open(cx, "dev", ConnectionState::Connected(client), None);
    open(cx, "staging", ConnectionState::Connecting, None);
    open(
        cx,
        "broken",
        ConnectionState::Failed("token=secret".into()),
        None,
    );

    let output = call(cx, &tools, "list_contexts", json!({})).await.unwrap();
    assert_eq!(
        Value::Object(output.content),
        json!({"contexts": [
            {"name": "broken", "status": "failed"},
            {"name": "dev", "status": "connected"},
            {"name": "prod", "status": "not_open"},
            {"name": "staging", "status": "connecting"},
        ]})
    );
}

#[gpui_kit::test]
async fn each_context_reports_only_its_own_discovery(cx: &mut TestAppContext) {
    let tools = tools(cx, &["dev", "ops"]);
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let (_dev_api, dev) = Recorder::start(&handle, "200 OK", json!({}));
    let (_ops_api, ops) = Recorder::start(&handle, "200 OK", json!({}));
    open(
        cx,
        "dev",
        ConnectionState::Connected(dev),
        Some(vec![discovered("", "v1", "Pod", "pods", true)]),
    );
    open(
        cx,
        "ops",
        ConnectionState::Connected(ops),
        Some(vec![discovered(
            "apps",
            "v1",
            "Deployment",
            "deployments",
            true,
        )]),
    );

    let output = call(cx, &tools, "list_resource_kinds", json!({"context": "dev"}))
        .await
        .unwrap();
    assert_eq!(kind_names(&output.content), ["Pod"]);
    assert_eq!(output.content["kinds"][0]["plural"], "pods");
    let output = call(cx, &tools, "list_resource_kinds", json!({"context": "ops"}))
        .await
        .unwrap();
    assert_eq!(kind_names(&output.content), ["Deployment"]);
}

#[gpui_kit::test]
async fn a_context_not_open_never_reaches_a_cluster(cx: &mut TestAppContext) {
    let tools = tools(cx, &["dev", "prod"]);
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let (recorder, client) = Recorder::start(&handle, "200 OK", json!({}));
    open(cx, "dev", ConnectionState::Failed("refused".into()), None);
    // A healthy session elsewhere, which none of these calls may touch.
    open(cx, "other", ConnectionState::Connected(client), None);

    for (context, expected) in [
        (
            "nope",
            ToolError::UnknownContext {
                context: "nope".into(),
            },
        ),
        (
            "prod",
            ToolError::Disconnected {
                context: "prod".into(),
            },
        ),
        (
            "dev",
            ToolError::Disconnected {
                context: "dev".into(),
            },
        ),
    ] {
        let result = call(
            cx,
            &tools,
            "list_resource_kinds",
            json!({"context": context}),
        )
        .await;
        assert_eq!(result, Err(expected), "{context}");
    }
    assert!(recorder.requests().is_empty());
    // And none of them opened a session.
    let open = cx.update(|cx| crate::k8s::cluster::session::ClusterRegistry::open_contexts(cx));
    assert!(!open.iter().any(|name| name == "nope" || name == "prod"));
}

#[gpui_kit::test]
async fn a_context_the_app_never_discovered_is_discovered_once(cx: &mut TestAppContext) {
    let tools = tools(cx, &["dev"]);
    let api = cluster_with_a_failing_aggregated_group();
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let client = {
        let _runtime = handle.enter();
        api.client()
    };
    open(cx, "dev", ConnectionState::Connected(client), None);

    let output = call(cx, &tools, "list_resource_kinds", json!({"context": "dev"}))
        .await
        .unwrap();
    assert_eq!(kind_names(&output.content), ["Pod", "Deployment"]);
    let published = cx.update(|cx| DiscoveryRegistry::existing_kinds(cx, "dev"));
    assert_eq!(published.map(|kinds| kinds.len()), Some(2));
}

#[gpui_kit::test]
async fn arguments_must_name_a_context(cx: &mut TestAppContext) {
    let tools = tools(cx, &[]);
    let result = call(cx, &tools, "list_resource_kinds", json!({})).await;
    assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    let result = call(cx, &tools, "list_contexts", json!({"context": "dev"})).await;
    assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
}
