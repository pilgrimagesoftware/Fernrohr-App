//! The tool list and the read tools, through the adapter.

use super::{Agent, content, error_code};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::test_cluster::FakeCluster;
use crate::mcp::read::test_support::{open, tools};
use crate::mcp::test_support::discovered;
use gpui_kit::TestAppContext;
use serde_json::json;

const READS: [&str; 5] = [
    "get_pod_logs",
    "get_resource",
    "list_contexts",
    "list_resource_kinds",
    "list_resources",
];
const NAVIGATION: [&str; 4] = [
    "connect_context",
    "list_layouts",
    "load_layout",
    "open_panel",
];
const ACTIONS: [&str; 8] = [
    "delete_pods",
    "restart_workload",
    "rollback_workload",
    "scale_workload",
    "set_configmap_value",
    "set_cronjob_suspended",
    "set_rollout_paused",
    "trigger_cronjob",
];

#[gpui_kit::test]
async fn an_agent_sees_every_tool_and_which_ones_change_a_cluster(cx: &mut TestAppContext) {
    let tools = tools(cx, &["dev"]);
    let mut agent = Agent::connect(cx, tools).await;

    let listed = agent.request(cx, "tools/list", json!({})).await;

    let listed = listed["result"]["tools"].as_array().expect("a tool list");
    let mut names: Vec<&str> = listed
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    names.sort_unstable();
    let mut expected: Vec<&str> = READS.into_iter().chain(NAVIGATION).chain(ACTIONS).collect();
    expected.sort_unstable();
    assert_eq!(names, expected, "these tools and no others");
    for tool in listed {
        let name = tool["name"].as_str().unwrap();
        let action = ACTIONS.contains(&name);
        assert_eq!(tool["annotations"]["readOnlyHint"], !action, "{name}");
        assert_eq!(tool["annotations"]["destructiveHint"], action, "{name}");
        assert_eq!(tool["inputSchema"]["type"], "object", "{name}");
    }
}

#[gpui_kit::test]
async fn an_agent_reads_a_connected_cluster_and_never_sees_a_secret_value(cx: &mut TestAppContext) {
    let tools = tools(cx, &["dev", "prod"]);
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply(
        "/api/v1",
        "pods",
        json!({"apiVersion": "v1", "kind": "Pod",
               "metadata": {"name": "api", "namespace": "team-a"},
               "spec": {"containers": [{"name": "app", "image": "nginx"}]}}),
    );
    cluster.apply(
        "/api/v1",
        "secrets",
        json!({"apiVersion": "v1", "kind": "Secret",
               "metadata": {"name": "db", "namespace": "team-a"},
               "data": {"password": "aHVudGVyMg=="}}),
    );
    open(
        cx,
        "dev",
        ConnectionState::Connected(client),
        Some(vec![
            discovered("", "v1", "Pod", "pods", true),
            discovered("", "v1", "Secret", "secrets", true),
        ]),
    );
    let mut agent = Agent::connect(cx, tools).await;

    let contexts = agent.call(cx, "list_contexts", json!({})).await;
    let contexts = content(&contexts)["contexts"].to_string();
    assert!(
        contexts.contains("\"dev\"") && contexts.contains("\"prod\""),
        "{contexts}"
    );

    let pod = agent
        .call(
            cx,
            "get_resource",
            json!({"context": "dev", "kind": "pod", "namespace": "team-a", "name": "api"}),
        )
        .await;
    assert!(content(&pod).to_string().contains("nginx"), "{pod}");

    let secret = agent
        .call(
            cx,
            "get_resource",
            json!({"context": "dev", "kind": "Secret", "namespace": "team-a", "name": "db"}),
        )
        .await;
    let wire = secret.to_string();
    assert!(!wire.contains("aHVudGVyMg=="), "{wire}");
    assert!(wire.contains("<redacted: 7 bytes>"), "{wire}");
}

#[gpui_kit::test]
async fn a_context_the_agent_cannot_use_is_a_tool_error(cx: &mut TestAppContext) {
    let tools = tools(cx, &["dev", "prod"]);
    open(cx, "dev", ConnectionState::Connecting, None);
    let mut agent = Agent::connect(cx, tools).await;

    for (context, code) in [
        ("nope", "unknown_context"),
        ("prod", "disconnected"),
        ("dev", "disconnected"),
    ] {
        let result = agent
            .call(
                cx,
                "list_resources",
                json!({"context": context, "kind": "pods"}),
            )
            .await;
        assert_eq!(error_code(&result), code, "{context}");
    }
}
