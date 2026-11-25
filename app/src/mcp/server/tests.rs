use super::*;
use crate::k8s::cluster::session::{ClusterRegistry, LiveAgents};
use crate::mcp::session::SessionId;
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

/// Opens a session on `server` reporting `client_name`, returning its
/// identifier and the connection that keeps it open.
async fn open_session(server: &TestServer, client_name: Option<&str>) -> (SessionId, UnixStream) {
    let token = server.files.token.expose().to_string();
    let (reply, mut stream) = handshake(server, hello_with(&token)).await;
    assert_eq!(reply, Some(HelloReply::Accepted));
    let open = Request::OpenSession {
        client_name: client_name.map(str::to_string),
    };
    frame::write(&mut stream, &open, MCP_MAX_REQUEST_BYTES)
        .await
        .unwrap();
    match frame::read::<Reply>(&mut stream, MCP_MAX_REPLY_BYTES)
        .await
        .unwrap()
    {
        Reply::Session { session } => (session, stream),
        other => panic!("expected a session, got {other:?}"),
    }
}

/// A request connection past the handshake.
async fn request_stream(server: &TestServer) -> UnixStream {
    let token = server.files.token.expose().to_string();
    let (reply, stream) = handshake(server, hello_with(&token)).await;
    assert_eq!(reply, Some(HelloReply::Accepted));
    stream
}

const TOKEN_REFUSED: Option<HelloReply> = Some(HelloReply::Rejected {
    reason: Rejection::Token,
});

#[tokio::test]
async fn the_current_token_is_accepted_and_lists_tools() {
    let server = TestServer::start(temp_endpoint_paths(), echo_tool_registry());
    let (session, _link) = open_session(&server, None).await;
    let mut stream = request_stream(&server).await;

    frame::write(
        &mut stream,
        &Request::ListTools { session },
        MCP_MAX_REQUEST_BYTES,
    )
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
    let open = Request::OpenSession { client_name: None };
    let _ = frame::write(&mut stream, &open, MCP_MAX_REQUEST_BYTES).await;
    assert!(matches!(
        frame::read::<Reply>(&mut stream, MCP_MAX_REPLY_BYTES).await,
        Err(FrameError::Closed | FrameError::Io(_))
    ));
}

/// Security pass: a connection past the handshake that never sends its
/// request is dropped once the handshake timeout passes, rather than holding
/// one of the [`MCP_MAX_CONNECTIONS`] slots for as long as it likes.
#[tokio::test]
async fn a_connection_that_sends_no_request_is_dropped() {
    let server = TestServer::start(temp_endpoint_paths(), echo_tool_registry());
    let mut stream = request_stream(&server).await;
    let read = tokio::time::timeout(
        MCP_HANDSHAKE_TIMEOUT + Duration::from_secs(5),
        frame::read::<Reply>(&mut stream, MCP_MAX_REPLY_BYTES),
    )
    .await
    .expect("the app drops the silent connection within the timeout");
    assert!(matches!(read, Err(FrameError::Closed | FrameError::Io(_))));
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
    let (session, _link) = open_session(&server, None).await;
    let mut stream = request_stream(&server).await;
    let call = Request::CallTool {
        session,
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
    let (session, _link) = open_session(&server, None).await;
    let mut stream = request_stream(&server).await;
    let call = Request::CallTool {
        session,
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

const NO_SESSION: Reply = Reply::Refused {
    error: ToolError::Unavailable {
        reason: Unavailability::NoSession,
    },
};

/// `agent-mcp` "Older adapter": a request naming no live session is refused
/// with the restart error, and the tool never runs.
#[tokio::test]
async fn a_request_without_a_live_session_is_refused_before_it_runs() {
    let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut registry = ToolRegistry::default();
    let flag = Arc::clone(&ran);
    registry.register(spec("mark"), move |_: Value, _| {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
        async { Ok(ToolOutput::new(Map::new())) }
    });
    let server = TestServer::start(temp_endpoint_paths(), registry);
    let forged = SessionId::generate().unwrap();
    for request in [
        Request::ListTools {
            session: forged.clone(),
        },
        Request::CallTool {
            session: forged.clone(),
            name: "mark".into(),
            arguments: Map::new(),
        },
    ] {
        let mut stream = request_stream(&server).await;
        frame::write(&mut stream, &request, MCP_MAX_REQUEST_BYTES)
            .await
            .unwrap();
        let reply: Reply = frame::read(&mut stream, MCP_MAX_REPLY_BYTES).await.unwrap();
        assert_eq!(reply, NO_SESSION);
    }
    assert!(
        !ran.load(std::sync::atomic::Ordering::SeqCst),
        "nothing ran"
    );
}

/// `agent-mcp` "Ended session": once a session's connection closes, its
/// identifier is refused like any unknown one.
#[tokio::test]
async fn a_closed_sessions_identifier_stops_working() {
    let server = TestServer::start(temp_endpoint_paths(), echo_tool_registry());
    let (session, link) = open_session(&server, None).await;
    drop(link);
    let refused = async {
        loop {
            let mut stream = request_stream(&server).await;
            let request = Request::ListTools {
                session: session.clone(),
            };
            frame::write(&mut stream, &request, MCP_MAX_REQUEST_BYTES)
                .await
                .unwrap();
            let reply: Reply = frame::read(&mut stream, MCP_MAX_REPLY_BYTES).await.unwrap();
            if reply == NO_SESSION {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(5), refused)
        .await
        .expect("the ended session's identifier is refused");
}

/// A call runs as its session's caller, carrying the name the client reported
/// (cleaned of control characters) - and that name is all it carries.
#[tokio::test]
async fn a_call_runs_as_its_sessions_caller() {
    let mut registry = ToolRegistry::default();
    registry.register(
        spec("whoami"),
        |_: Value, context: ToolContext| async move {
            let name = context.caller.and_then(|caller| caller.client_name);
            Ok(ToolOutput::new(object(json!({ "name": name }))))
        },
    );
    let server = TestServer::start(temp_endpoint_paths(), registry);
    let (session, _link) = open_session(&server, Some("assistant\u{7}\n")).await;
    let mut stream = request_stream(&server).await;
    let call = Request::CallTool {
        session,
        name: "whoami".into(),
        arguments: Map::new(),
    };
    frame::write(&mut stream, &call, MCP_MAX_REQUEST_BYTES)
        .await
        .unwrap();
    let reply: Reply = frame::read(&mut stream, MCP_MAX_REPLY_BYTES).await.unwrap();
    assert_eq!(
        reply,
        Reply::Called {
            result: Ok(ToolOutput::new(object(json!({"name": "assistant"}))))
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

/// `agent-mcp` "Client exits", end to end: a session holds a context (as
/// `connect_context` will), its adapter goes away, and the hold lingers on the
/// app's main thread for the grace period - no window or session holds it -
/// then the connection is released.
#[gpui_kit::test]
async fn an_ended_sessions_holds_linger_then_go(cx: &mut gpui_kit::TestAppContext) {
    use super::super::client::{AppClient, Call};
    use crate::k8s::cluster::connection::ConnectionState;
    use crate::k8s::cluster::session::Holder;
    use crate::mcp::test_support::test_context;

    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
    });
    let mut registry = ToolRegistry::default();
    registry.register(
        spec("hold_demo"),
        |_: Value, context: ToolContext| async move {
            let agent = context.caller.expect("called in a session").agent;
            context
                .foreground
                .run(move |cx| ClusterRegistry::hold(cx, "demo", agent))
                .await?;
            Ok(ToolOutput::new(Map::new()))
        },
    );
    let context = ToolContext {
        foreground: cx.update(super::super::foreground::Foreground::spawn_on),
        ..test_context()
    };
    let runtime = cx.update(|cx| crate::runtime::handle(cx));
    let paths = temp_endpoint_paths();
    let _app = {
        let _entered = runtime.enter();
        TestServer::start_with(paths.clone(), registry, context)
    };

    let client = AppClient::new(paths);
    let call = Call::CallTool {
        name: "hold_demo".into(),
        arguments: Map::new(),
    };
    let calling = runtime.spawn(async move {
        let reply = client.request(&call, Some("assistant")).await;
        (client, reply)
    });
    let (client, reply) = loop {
        cx.run_until_parked();
        if calling.is_finished() {
            break calling.await.expect("the call ran");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(
        matches!(reply, Ok(Reply::Called { result: Ok(_) })),
        "{reply:?}"
    );
    assert!(matches!(
        cx.update(|cx| ClusterRegistry::holders(cx, "demo"))
            .as_slice(),
        [Holder::Agent(_)]
    ));

    // The adapter exits: its session link closes.
    drop(client);
    for _ in 0..400 {
        cx.run_until_parked();
        if matches!(
            cx.update(|cx| ClusterRegistry::holders(cx, "demo"))
                .as_slice(),
            [Holder::Lingering(_)]
        ) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        matches!(
            cx.update(|cx| ClusterRegistry::holders(cx, "demo"))
                .as_slice(),
            [Holder::Lingering(_)]
        ),
        "the ended session's hold lingers"
    );
    assert!(cx.update(|cx| ClusterRegistry::existing_connection(cx, "demo").is_some()));

    cx.executor()
        .advance_clock(MCP_SESSION_GRACE + Duration::from_secs(1));
    cx.run_until_parked();
    assert!(
        cx.update(|cx| ClusterRegistry::existing_connection(cx, "demo").is_none()),
        "released after the grace period"
    );
}

/// `agent-mcp` "Stop serving with the last window": stopping ends every
/// session - its id stops working and its holds linger, then go.
#[gpui_kit::test]
async fn stopping_ends_every_session(cx: &mut gpui_kit::TestAppContext) {
    use crate::k8s::cluster::connection::ConnectionState;
    use crate::k8s::cluster::session::Holder;
    use crate::mcp::session::Sessions;

    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
    });
    let sessions = Sessions::default();
    let (id, caller) = sessions.open(Some("assistant".into())).unwrap();
    // A second agent whose own end was cut short (review of App#434, F2): the
    // endpoint already closed its session, but its end never reached the main
    // thread, so it is still live and still holds "demo".
    let (_, cut_short) = sessions.open(None).unwrap();
    cx.update(|cx| {
        for agent in [caller.agent, cut_short.agent] {
            LiveAgents::open(cx, agent, None);
            assert!(ClusterRegistry::hold(cx, "demo", agent));
        }
    });
    sessions.close(cut_short.agent);

    cx.update(|cx| end_every_session(cx, &sessions, MCP_SESSION_GRACE));
    assert!(sessions.caller(&id).is_none(), "its id stopped working");
    let holders = cx.update(|cx| ClusterRegistry::holders(cx, "demo"));
    assert!(
        holders
            .iter()
            .all(|holder| matches!(holder, Holder::Lingering(_))),
        "both agents' holds linger, the cut-short one's too: {holders:?}"
    );
    assert!(
        !cx.update(|cx| LiveAgents::is_live(cx, cut_short.agent)),
        "and neither is live any more"
    );
    cx.executor()
        .advance_clock(MCP_SESSION_GRACE + Duration::from_secs(1));
    cx.run_until_parked();
    assert!(cx.update(|cx| ClusterRegistry::existing_connection(cx, "demo").is_none()));
}
