use super::*;
use crate::mcp::test_support::{TestServer, echo_tool_registry, object, temp_endpoint_paths};
use crate::mcp::tools::{ToolKind, ToolOutput, ToolSpec};
use serde_json::{Map, Value, json};
use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;

/// Connects to `server` and sends `hello` as raw JSON, returning the reply and
/// the still-open stream.
async fn handshake(server: &TestServer, hello: Value) -> (Option<HelloReply>, UnixStream) {
    let mut stream = UnixStream::connect(&server.files.paths.socket)
        .await
        .unwrap();
    frame::write(&mut stream, &hello, HELLO_LIMIT)
        .await
        .unwrap();
    let reply = frame::read::<HelloReply>(&mut stream, HELLO_LIMIT)
        .await
        .ok();
    (reply, stream)
}

fn hello_with(token: &str) -> Value {
    json!({"protocol": PROTOCOL_VERSION, "token": token})
}

const TOKEN_REFUSED: Option<HelloReply> = Some(HelloReply::Rejected {
    reason: Rejection::Token,
});

#[tokio::test]
async fn the_current_token_is_accepted_and_lists_tools() {
    let server = TestServer::start(temp_endpoint_paths(), echo_tool_registry());
    let token = server.files.token.expose().to_string();
    let (reply, mut stream) = handshake(&server, hello_with(&token)).await;
    assert_eq!(reply, Some(HelloReply::Accepted));

    frame::write(&mut stream, &Request::ListTools, MCP_MAX_REQUEST_BYTES)
        .await
        .unwrap();
    let reply: Reply = frame::read(&mut stream, MCP_MAX_REPLY_BYTES).await.unwrap();
    let Reply::Tools { tools } = reply else {
        panic!("expected a tool list, got {reply:?}");
    };
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "echo");
}

#[tokio::test]
async fn a_hello_without_a_token_is_refused() {
    let server = TestServer::start(temp_endpoint_paths(), echo_tool_registry());
    let (reply, _) = handshake(&server, json!({"protocol": PROTOCOL_VERSION})).await;
    assert_eq!(reply, TOKEN_REFUSED);
}

#[tokio::test]
async fn a_wrong_token_is_refused() {
    let server = TestServer::start(temp_endpoint_paths(), echo_tool_registry());
    let (reply, _) = handshake(&server, hello_with(&"0".repeat(64))).await;
    assert_eq!(reply, TOKEN_REFUSED);
    let (reply, _) = handshake(&server, hello_with("")).await;
    assert_eq!(reply, TOKEN_REFUSED);
}

#[tokio::test]
async fn a_previous_launchs_token_is_refused() {
    let paths = temp_endpoint_paths();
    let first = TestServer::start(paths.clone(), echo_tool_registry());
    let stale = first.files.token.expose().to_string();
    drop(first);

    let second = TestServer::start(paths, echo_tool_registry());
    let (reply, _) = handshake(&second, hello_with(&stale)).await;
    assert_eq!(reply, TOKEN_REFUSED);
}

#[tokio::test]
async fn another_protocol_version_is_refused() {
    let server = TestServer::start(temp_endpoint_paths(), echo_tool_registry());
    let token = server.files.token.expose().to_string();
    let hello = json!({"protocol": PROTOCOL_VERSION + 1, "token": token});
    let (reply, _) = handshake(&server, hello).await;
    assert_eq!(
        reply,
        Some(HelloReply::Rejected {
            reason: Rejection::Protocol
        })
    );
}

#[tokio::test]
async fn a_refused_handshake_gets_no_request_served() {
    let server = TestServer::start(temp_endpoint_paths(), echo_tool_registry());
    let (_, mut stream) = handshake(&server, hello_with("wrong")).await;
    // The app has closed its end: a request goes nowhere and nothing answers.
    let _ = frame::write(&mut stream, &Request::ListTools, MCP_MAX_REQUEST_BYTES).await;
    assert!(matches!(
        frame::read::<Reply>(&mut stream, MCP_MAX_REPLY_BYTES).await,
        Err(FrameError::Closed | FrameError::Io(_))
    ));
}

#[tokio::test]
async fn hanging_up_cancels_a_running_call() {
    let (dropped_tx, dropped_rx) = oneshot::channel::<()>();
    let dropped_tx = parking_lot::Mutex::new(Some(dropped_tx));
    let mut registry = ToolRegistry::default();
    registry.register(spec("wait"), move |_: Value, _| {
        // Fires when the call's future is dropped, never by finishing.
        let guard = DropSignal(dropped_tx.lock().take());
        async move {
            let _guard = guard;
            std::future::pending::<()>().await;
            Ok(ToolOutput::new(Map::new()))
        }
    });
    let server = TestServer::start(temp_endpoint_paths(), registry);
    let token = server.files.token.expose().to_string();
    let (_, mut stream) = handshake(&server, hello_with(&token)).await;
    let call = Request::CallTool {
        name: "wait".into(),
        arguments: Map::new(),
    };
    frame::write(&mut stream, &call, MCP_MAX_REQUEST_BYTES)
        .await
        .unwrap();
    stream.shutdown().await.unwrap();
    drop(stream);

    tokio::time::timeout(Duration::from_secs(5), dropped_rx)
        .await
        .expect("the call was cancelled")
        .unwrap();
}

#[tokio::test]
async fn an_oversized_result_is_replaced_by_an_error() {
    let mut registry = ToolRegistry::default();
    registry.register(spec("huge"), |_: Value, _| async {
        Ok(ToolOutput::new(object(
            json!({"data": "x".repeat(MCP_MAX_REPLY_BYTES)}),
        )))
    });
    let server = TestServer::start(temp_endpoint_paths(), registry);
    let token = server.files.token.expose().to_string();
    let (_, mut stream) = handshake(&server, hello_with(&token)).await;
    let call = Request::CallTool {
        name: "huge".into(),
        arguments: Map::new(),
    };
    frame::write(&mut stream, &call, MCP_MAX_REQUEST_BYTES)
        .await
        .unwrap();
    let reply: Reply = frame::read(&mut stream, MCP_MAX_REPLY_BYTES).await.unwrap();
    assert_eq!(
        reply,
        Reply::Called {
            result: Err(ToolError::ResultTooLarge {
                limit: MCP_MAX_REPLY_BYTES
            })
        }
    );
}

fn spec(name: &str) -> ToolSpec {
    ToolSpec {
        name: name.into(),
        title: name.into(),
        description: String::new(),
        kind: ToolKind::Read,
        input_schema: object(json!({"type": "object"})),
    }
}

struct DropSignal(Option<oneshot::Sender<()>>);

impl Drop for DropSignal {
    fn drop(&mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(());
        }
    }
}
