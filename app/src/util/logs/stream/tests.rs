// Not `use super::*`: `logs.rs` pulls in `gpui_kit::*`, whose own `test`
// attribute macro would shadow `core::prelude::v1::test` for the plain
// synchronous tests below.
use super::{describe_log_stream_error, start_stream};
use crate::util::logs::{LogEvent, LogsView};
use gpui_kit::{AppContext as _, TestAppContext};

#[gpui_kit::test]
async fn mock_stream_populates_history_line_by_line(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let view = cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        cx.new(|_| LogsView::new(vec!["app".into()]))
    });

    let task = cx.update(|cx| {
        start_stream(view.clone(), cx, 8, |tx| async move {
            for line in ["line one", "line two", "line three"] {
                tx.send(LogEvent::Line(line.into())).await.unwrap();
            }
        })
    });
    task.await;

    let lines = view.read_with(cx, |view, _| view.lines().to_vec());
    assert_eq!(lines, vec!["line one", "line two", "line three"]);
}

/// A fixture `kube::Error::Api` with `code`, `reason`, and `message` filled in
/// - the shape `describe_log_stream_error` reads.
fn api_error(code: u16, reason: &str, message: &str) -> kube::Error {
    kube::Error::Api(Box::new(kube::core::response::Status {
        code,
        reason: reason.to_string(),
        message: message.to_string(),
        ..Default::default()
    }))
}

/// `1-window-context-bar` bug 2: `kube::Error`'s own `Display` for a 404 is
/// `ApiError: pods "clamav-plc9g" not found (Api(Status { ... }))` - readable to
/// a developer, not a user. The pod-not-found case reads as a plain sentence
/// naming the pod, its namespace, and the context instead.
#[test]
fn describe_log_stream_error_names_the_pod_for_a_404() {
    let error = api_error(404, "NotFound", "pods \"clamav-plc9g\" not found");

    assert_eq!(
        describe_log_stream_error(&error, "clamav-plc9g", "default", "northbay"),
        "Pod clamav-plc9g not found in namespace default on northbay."
    );
}

/// Every other `ApiError` reads as `<reason>: <message>` - the API's own words,
/// not the client library's `ApiError: <msg> (<Debug>)` wrapper around them.
#[test]
fn describe_log_stream_error_reads_reason_and_message_for_other_api_errors() {
    let error = api_error(
        403,
        "Forbidden",
        "pods is forbidden: User \"x\" cannot list resource",
    );

    assert_eq!(
        describe_log_stream_error(&error, "web-1", "default", "northbay"),
        "Forbidden: pods is forbidden: User \"x\" cannot list resource"
    );
}

fn target(follow: bool, previous: bool) -> super::LogTarget {
    super::LogTarget {
        namespace: "jobs".into(),
        pod_name: "migrate-x7k2p".into(),
        container: "main".into(),
        context_name: "dev".into(),
        previous,
        tail_lines: None,
        follow,
        since: None,
    }
}

/// `completed-pod-logs` 2.1: a running container is followed; a terminated
/// one, and a previous instance, are read to their end.
#[test]
fn log_params_follow_only_a_live_current_instance() {
    use super::log_params;
    assert!(
        log_params(&target(true, false)).follow,
        "a running container"
    );
    assert!(
        !log_params(&target(false, false)).follow,
        "a terminated one"
    );
    let previous = log_params(&target(true, true));
    assert!(!previous.follow && previous.previous, "a previous instance");
}

fn pod(phase: &str, state: Option<&str>) -> k8s_openapi::api::core::v1::Pod {
    let state = match state {
        Some("terminated") => serde_json::json!({"terminated": {"exitCode": 0}}),
        Some("running") => serde_json::json!({"running": {}}),
        Some("waiting") => serde_json::json!({"waiting": {"reason": "CrashLoopBackOff"}}),
        _ => serde_json::Value::Null,
    };
    let statuses = if state.is_null() {
        serde_json::json!([])
    } else {
        serde_json::json!([{
            "name": "main", "ready": false, "restartCount": 0,
            "image": "busybox", "imageID": "", "state": state,
        }])
    };
    serde_json::from_value(serde_json::json!({
        "metadata": {"name": "migrate-x7k2p", "namespace": "jobs"},
        "status": {"phase": phase, "containerStatuses": statuses},
    }))
    .expect("a valid pod")
}

/// `completed-pod-logs` 2.2: which containers' logs are followed.
#[test]
fn a_terminated_container_or_finished_pod_is_not_followed() {
    use super::follows;
    assert!(!follows(Some(&pod("Running", Some("terminated"))), "main"));
    assert!(!follows(Some(&pod("Succeeded", None)), "main"));
    assert!(!follows(Some(&pod("Failed", None)), "main"));
    assert!(follows(Some(&pod("Running", Some("running"))), "main"));
    assert!(
        follows(Some(&pod("Running", Some("waiting"))), "main"),
        "a restart-looping container still streams"
    );
    assert!(
        follows(Some(&pod("Pending", None)), "main"),
        "no status yet"
    );
    assert!(follows(None, "main"), "an unknown pod: today's behaviour");
    assert!(
        follows(Some(&pod("Running", Some("terminated"))), "sidecar"),
        "only the named container's status counts"
    );
}

/// `completed-pod-logs` 2.3: a terminated container's log, read without
/// following, arrives whole - every line - before the stream's end.
#[test]
fn a_terminated_containers_whole_log_arrives_before_the_end() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let (server, client) = crate::k8s::test_recorder::Recorder::start_text(
        runtime.handle(),
        "migrating\napplied 3 migrations\ndone\n",
    );
    let events = runtime.block_on(async move {
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        super::produce_container_logs(client, target(false, false), tx).await;
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(match event {
                LogEvent::Line(line) => line,
                LogEvent::Ended => "<ended>".into(),
                LogEvent::RequestFailed { message, .. } => format!("<failed: {message}>"),
                LogEvent::NoPreviousInstance => "<no previous>".into(),
            });
        }
        events
    });
    assert_eq!(
        events,
        ["migrating", "applied 3 migrations", "done", "<ended>"]
    );
    let request = &server.requests()[0];
    assert!(
        request.target.contains("/pods/migrate-x7k2p/log"),
        "{}",
        request.target
    );
    assert!(
        !request.target.contains("follow=true"),
        "read to its end, not followed: {}",
        request.target
    );
}

/// #238 review: a quiet container's stream - the request answered, no line
/// coming - stops as soon as nobody listens. It used to notice only at its
/// next send, so its connection and task outlived the panel's stream.
#[test]
fn a_quiet_stream_stops_once_its_receiver_is_gone() {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a port");
        let addr = listener.local_addr().expect("an address");
        let (answered_tx, answered) = tokio::sync::oneshot::channel();
        // Answers the log request with headers, then holds the connection
        // open without a line, as a quiet container's log does.
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("the request");
            let mut request = [0u8; 4096];
            let _ = socket.read(&mut request).await;
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\
                      Transfer-Encoding: chunked\r\n\r\n",
                )
                .await
                .expect("the headers");
            let _ = answered_tx.send(());
            std::future::pending::<()>().await;
            drop(socket);
        });
        let client = kube::Client::try_from(kube::Config::new(
            format!("http://{addr}").parse().expect("a url"),
        ))
        .expect("a client");
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let producer = tokio::spawn(super::produce_container_logs(
            client,
            target(true, false),
            tx,
        ));
        answered.await.expect("the server answered");
        drop(rx);
        tokio::time::timeout(std::time::Duration::from_secs(5), producer)
            .await
            .expect("the producer stops once the receiver is gone")
            .expect("and doesn't panic");
    });
}

/// #238 review: a resumed stream's request asks for what was written since,
/// not a tail - the API's way to leave out lines the view already shows.
#[test]
fn a_resumed_request_asks_since_without_a_tail() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let (server, client) =
        crate::k8s::test_recorder::Recorder::start_text(runtime.handle(), "later\n");
    let mut resumed = target(true, false);
    resumed.since = Some("2026-10-12T18:00:01Z".parse().unwrap());
    runtime.block_on(async move {
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        super::produce_container_logs(client, resumed, tx).await;
        while rx.recv().await.is_some() {}
    });
    let request = &server.requests()[0].target;
    assert!(
        request.contains("sinceTime=2026-10-12T18%3A00%3A01Z"),
        "{request}"
    );
    assert!(!request.contains("tailLines"), "{request}");
}
