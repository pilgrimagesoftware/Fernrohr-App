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
        gpui_kit::init(cx);
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
