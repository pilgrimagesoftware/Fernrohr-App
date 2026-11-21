//! The adapter's session (`mcp-connections` design decision 3): one per
//! adapter, opened on the first request and reused, carrying the client's
//! reported name, and replaced - never resumed - when the app restarts.

use super::{AppClient, Call};
use crate::mcp::protocol::Reply;
use crate::mcp::test_support::{TestServer, echo_tool_registry, object, temp_endpoint_paths};
use crate::mcp::tools::{ToolContext, ToolKind, ToolOutput, ToolRegistry, ToolSpec};
use serde_json::{Value, json};

async fn session_of(client: &AppClient) -> Option<super::SessionId> {
    client
        .session
        .lock()
        .await
        .as_ref()
        .map(|open| open.id.clone())
}

fn echo(text: &str) -> Call {
    Call::CallTool {
        name: "echo".into(),
        arguments: object(json!({ "text": text })),
    }
}

#[tokio::test]
async fn one_session_serves_every_request() {
    let paths = temp_endpoint_paths();
    let _app = TestServer::start(paths.clone(), echo_tool_registry());
    let client = AppClient::new(paths);
    assert!(session_of(&client).await.is_none(), "opened on first use");

    client.request(&Call::ListTools, None).await.unwrap();
    let first = session_of(&client)
        .await
        .expect("the first request opened one");
    client.request(&echo("hi"), None).await.unwrap();
    assert_eq!(
        session_of(&client).await,
        Some(first),
        "reused, not reopened"
    );
}

#[tokio::test]
async fn the_client_name_reaches_the_app() {
    let mut registry = ToolRegistry::default();
    registry.register(
        ToolSpec {
            name: "whoami".into(),
            title: "whoami".into(),
            description: String::new(),
            kind: ToolKind::Read,
            input_schema: object(json!({"type": "object"})),
        },
        |_: Value, context: ToolContext| async move {
            let name = context.caller.and_then(|caller| caller.client_name);
            Ok(ToolOutput::new(object(json!({ "name": name }))))
        },
    );
    let paths = temp_endpoint_paths();
    let _app = TestServer::start(paths.clone(), registry);
    let client = AppClient::new(paths);
    let call = Call::CallTool {
        name: "whoami".into(),
        arguments: Default::default(),
    };
    let reply = client.request(&call, Some("assistant")).await.unwrap();
    assert_eq!(
        reply,
        Reply::Called {
            result: Ok(ToolOutput::new(object(json!({"name": "assistant"}))))
        }
    );
}

/// The app restarting ends the adapter's session; the next request opens a
/// new one - a different identifier, so nothing of the old session carries
/// over - and succeeds.
#[tokio::test]
async fn an_app_restart_gets_a_new_session_not_the_old_one() {
    let paths = temp_endpoint_paths();
    let first = TestServer::start(paths.clone(), echo_tool_registry());
    let client = AppClient::new(paths.clone());
    client.request(&echo("before"), None).await.unwrap();
    let before = session_of(&client).await.unwrap();
    drop(first);

    let _second = TestServer::start(paths, echo_tool_registry());
    let reply = client.request(&echo("after"), None).await.unwrap();
    assert!(
        matches!(reply, Reply::Called { result: Ok(_) }),
        "{reply:?}"
    );
    let after = session_of(&client).await.unwrap();
    assert_ne!(after, before, "a fresh session, never the old one");
}

/// Every log line written in this test binary, captured once the first test
/// asks for it. Lines from other tests mix in; the assertion below only
/// looks for one secret, so that's harmless.
fn captured_logs() -> &'static parking_lot::Mutex<Vec<String>> {
    struct Capture;
    static LINES: parking_lot::Mutex<Vec<String>> = parking_lot::Mutex::new(Vec::new());
    impl log::Log for Capture {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            LINES
                .lock()
                .push(format!("{} {}", record.target(), record.args()));
        }
        fn flush(&self) {}
    }
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| {
        if log::set_logger(&Capture).is_ok() {
            log::set_max_level(log::LevelFilter::Trace);
        }
    });
    &LINES
}

/// `agent-mcp` "Identifier stays out of logs": a session opens, makes a
/// request and ends with every level logged, and no line holds its id.
#[tokio::test]
async fn a_sessions_identifier_never_reaches_the_log() {
    let lines = captured_logs();
    let paths = temp_endpoint_paths();
    let app = TestServer::start(paths.clone(), echo_tool_registry());
    let client = AppClient::new(paths);
    client
        .request(&echo("hi"), Some("assistant"))
        .await
        .unwrap();
    let id = session_of(&client).await.unwrap();
    // The id's text, as it crosses the wire.
    let secret = serde_json::to_value(&id)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    drop(client);
    // Wait for the app to log the session's end.
    for _ in 0..400 {
        if lines
            .lock()
            .iter()
            .any(|line| line.contains("session ended"))
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    drop(app);
    let lines = lines.lock();
    assert!(
        lines.iter().any(|line| line.contains("session opened")),
        "the session was logged: {lines:?}"
    );
    assert!(
        lines.iter().all(|line| !line.contains(&secret)),
        "no line holds the session id"
    );
}
