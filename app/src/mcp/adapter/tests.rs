//! The adapter as an MCP client sees it: newline-delimited JSON-RPC over an
//! in-memory pipe in place of stdio, against an app endpoint served in-process
//! (the fixture `echo` tool) or against no app at all.

use super::*;
use crate::mcp::test_support::{McpClient, TestServer, echo_tool_registry, temp_endpoint_paths};

#[tokio::test]
async fn a_client_lists_the_running_apps_tools() {
    let server = TestServer::start(temp_endpoint_paths(), echo_tool_registry());
    let mut client = McpClient::connect(server.files.paths.clone()).await;

    let response = client.request("tools/list", json!({})).await;
    let tools = response["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "echo");
    assert_eq!(tools[0]["title"], "Echo");
    assert_eq!(tools[0]["inputSchema"]["required"], json!(["text"]));
    assert_eq!(tools[0]["annotations"]["readOnlyHint"], true);
}

#[tokio::test]
async fn a_client_calls_a_tool_and_gets_its_structured_result() {
    let server = TestServer::start(temp_endpoint_paths(), echo_tool_registry());
    let mut client = McpClient::connect(server.files.paths.clone()).await;

    let response = client
        .request(
            "tools/call",
            json!({"name": "echo", "arguments": {"text": "hi"}}),
        )
        .await;
    let result = &response["result"];
    assert_eq!(result["isError"], false);
    assert_eq!(result["structuredContent"], json!({"echo": "hi"}));
}

#[tokio::test]
async fn bad_arguments_are_a_tool_error_the_model_can_read() {
    let server = TestServer::start(temp_endpoint_paths(), echo_tool_registry());
    let mut client = McpClient::connect(server.files.paths.clone()).await;

    let response = client
        .request("tools/call", json!({"name": "echo", "arguments": {}}))
        .await;
    let result = &response["result"];
    assert_eq!(result["isError"], true);
    assert_eq!(
        result["structuredContent"]["error"]["code"],
        "invalid_arguments"
    );
}

#[tokio::test]
async fn an_unknown_tool_is_a_protocol_error() {
    let server = TestServer::start(temp_endpoint_paths(), echo_tool_registry());
    let mut client = McpClient::connect(server.files.paths.clone()).await;

    let response = client
        .request("tools/call", json!({"name": "apply_yaml", "arguments": {}}))
        .await;
    assert_eq!(response["error"]["code"], ErrorCode::INVALID_PARAMS.0);
    assert_eq!(response["error"]["data"]["code"], "unknown_tool");
}

#[tokio::test]
async fn with_no_app_running_every_request_is_unavailable() {
    // An empty endpoint directory: no socket, no token, no app.
    let mut client = McpClient::connect(temp_endpoint_paths()).await;

    let listed = client.request("tools/list", json!({})).await;
    assert_eq!(listed["error"]["data"]["code"], "unavailable");
    assert_eq!(listed["error"]["data"]["reason"], "not_running");

    let called = client
        .request(
            "tools/call",
            json!({"name": "echo", "arguments": {"text": "x"}}),
        )
        .await;
    assert_eq!(called["result"]["isError"], true);
    assert_eq!(
        called["result"]["structuredContent"]["error"]["code"],
        "unavailable"
    );
}

#[tokio::test]
async fn an_app_started_mid_session_is_picked_up() {
    let paths = temp_endpoint_paths();
    let mut client = McpClient::connect(paths.clone()).await;
    let before = client.request("tools/list", json!({})).await;
    assert_eq!(before["error"]["data"]["code"], "unavailable");

    let _server = TestServer::start(paths, echo_tool_registry());
    let after = client.request("tools/list", json!({})).await;
    assert_eq!(after["result"]["tools"][0]["name"], "echo");
}

/// `serve` on an in-memory pipe whose client end the test keeps open, so only
/// `shutdown` can end it. Returns the client end and the running `serve`.
fn serve_until(
    shutdown: tokio::sync::oneshot::Receiver<Stop>,
) -> (
    tokio::io::DuplexStream,
    tokio::task::JoinHandle<Result<Stop, String>>,
) {
    let (ours, theirs) = tokio::io::duplex(1 << 16);
    let shutdown = async move { shutdown.await.unwrap_or(Stop::Closed) };
    let task = tokio::spawn(serve(
        AppClient::new(temp_endpoint_paths()),
        tokio::io::split(theirs),
        shutdown,
    ));
    (ours, task)
}

async fn ended_within_a_second(
    task: tokio::task::JoinHandle<Result<Stop, String>>,
) -> Result<Stop, String> {
    tokio::time::timeout(std::time::Duration::from_secs(1), task)
        .await
        .expect("serve should end promptly")
        .expect("serve should not panic")
}

#[tokio::test]
async fn shutdown_ends_an_initialized_session_with_stdin_still_open() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let (fire, shutdown) = tokio::sync::oneshot::channel();
    let (ours, task) = serve_until(shutdown);
    let (reader, mut writer) = tokio::io::split(ours);
    let initialize = json!({
        "jsonrpc": "2.0", "id": 0, "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "fixture", "version": "0"},
        },
    });
    writer
        .write_all(format!("{initialize}\n").as_bytes())
        .await
        .unwrap();
    let mut lines = BufReader::new(reader).lines();
    let reply = lines.next_line().await.unwrap().unwrap();
    assert!(reply.contains("\"serverInfo\""), "{reply}");

    fire.send(Stop::Signal(15)).unwrap();
    assert_eq!(ended_within_a_second(task).await, Ok(Stop::Signal(15)));
    drop(writer);
}

#[tokio::test]
async fn shutdown_ends_a_session_still_waiting_for_its_handshake() {
    let (fire, shutdown) = tokio::sync::oneshot::channel();
    let (_ours, task) = serve_until(shutdown);

    fire.send(Stop::ParentGone).unwrap();
    assert_eq!(ended_within_a_second(task).await, Ok(Stop::ParentGone));
}

#[test]
fn exit_codes_follow_the_shell_convention() {
    assert_eq!(Stop::Closed.exit_code(), 0);
    assert_eq!(Stop::ParentGone.exit_code(), 0);
    assert_eq!(Stop::Signal(15).exit_code(), 143);
    assert_eq!(Stop::Signal(1).exit_code(), 129);
}
