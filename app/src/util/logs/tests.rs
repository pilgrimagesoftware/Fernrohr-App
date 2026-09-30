//! Unit tests for `util::logs`: the pure `LogsView` state machine, the
//! readable-message/technical-detail split behind a failed stream, and
//! `LogsPanel`'s filtering of the app-scoped `SelectedPod` global by context.
//!
//! Not `use super::*`: `logs.rs` pulls in `gpui_kit::*`, whose own `test`
//! attribute macro would shadow `core::prelude::v1::test` for the plain
//! synchronous tests below.
use super::{
    FollowState, LogEvent, LogsPanel, LogsView, describe_log_stream_error, start_stream,
    streaming_title,
};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::{PodSelection, SelectedPod};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::{AppContext as _, TestAppContext};

#[test]
fn streaming_title_names_the_pod_and_container() {
    let current = (
        "default".to_string(),
        "web-1".to_string(),
        "app".to_string(),
    );
    assert_eq!(
        streaming_title(Some(&current), || "unreachable".to_string()),
        "Logs: web-1 · app"
    );
}

#[test]
fn streaming_title_falls_back_before_a_pod_is_selected() {
    assert_eq!(streaming_title(None, || "Logs".to_string()), "Logs");
}

#[test]
fn history_renders_and_new_lines_append() {
    let mut view = LogsView::new(vec!["app".into()]);
    view.apply(LogEvent::Line("first".into()));
    view.apply(LogEvent::Line("second".into()));

    assert_eq!(view.lines(), &["first".to_string(), "second".to_string()]);
}

#[test]
fn scrolling_up_pauses_and_returning_to_bottom_resumes() {
    let mut view = LogsView::new(vec!["app".into()]);
    assert_eq!(view.follow_state(), FollowState::Following);

    view.scroll_up();
    assert_eq!(view.follow_state(), FollowState::Paused);

    view.scroll_to_bottom();
    assert_eq!(view.follow_state(), FollowState::Following);
}

#[test]
fn single_container_needs_no_pick() {
    let view = LogsView::new(vec!["app".into()]);
    assert_eq!(view.selected_container(), "app");
    assert!(!view.needs_container_picker());
}

#[test]
fn multi_container_fixture_switches_streams_on_selection() {
    let mut view = LogsView::new(vec!["app".into(), "sidecar".into()]);
    assert_eq!(view.selected_container(), "app");
    assert!(view.needs_container_picker());

    view.apply(LogEvent::Line("from app".into()));
    let changed = view.select_container("sidecar");

    assert!(
        changed,
        "selecting a different container should report a change"
    );
    assert_eq!(view.selected_container(), "sidecar");
    assert!(
        view.lines().is_empty(),
        "switching containers should clear the previous container's history"
    );

    let unchanged = view.select_container("sidecar");
    assert!(!unchanged, "reselecting the current container is a no-op");
}

/// `1-window-context-bar` bug 2: the terminal state now carries a readable
/// message *and* the failure's full technical detail, rather than folding the
/// latter into the former (or discarding it).
#[test]
fn deleted_pod_and_not_started_container_show_distinct_terminal_messages() {
    let mut stream_ended = LogsView::new(vec!["app".into()]);
    stream_ended.apply(LogEvent::Line("some log line".into()));
    stream_ended.apply(LogEvent::Ended);

    let mut request_failed = LogsView::new(vec!["app".into()]);
    request_failed.apply(LogEvent::RequestFailed {
        message: "container not started".into(),
        detail: "RequestFailed(...)".into(),
    });

    let ended_message = stream_ended.terminal_message().unwrap();
    let failed_message = request_failed.terminal_message().unwrap();
    assert_ne!(ended_message, failed_message);
    assert!(ended_message.to_lowercase().contains("ended"));
    assert!(
        failed_message
            .to_lowercase()
            .contains("container not started")
    );
    assert!(
        stream_ended.terminal_detail().is_none(),
        "a normal end has no technical detail to show"
    );
    assert_eq!(
        request_failed.terminal_detail(),
        Some("RequestFailed(...)"),
        "the full technical detail stays available alongside the readable message"
    );
}

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
        describe_log_stream_error(&error, "clamav-plc9g", "default", "carefulcrab"),
        "Pod clamav-plc9g not found in namespace default on carefulcrab."
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
        describe_log_stream_error(&error, "web-1", "default", "carefulcrab"),
        "Forbidden: pods is forbidden: User \"x\" cannot list resource"
    );
}

/// `1-window-context-bar` bug 1's third root cause: `LogsPanel::sync` used to
/// re-sync to *any* `SelectedPod`, from any context - streaming whichever pod's
/// row was clicked last against its own scope's context, regardless of which
/// context actually held that pod. A selection published by a different
/// context must leave this panel exactly as it was: nothing selected, nothing
/// streamed.
///
/// `ConnectionState::Connecting` rather than `Connected`: the context check
/// runs (and must reject the foreign selection) before `sync` ever looks at
/// the connection, so nothing here needs a real client or spawns a stream
/// task for the test to outlive.
#[gpui_kit::test]
async fn a_logs_panel_ignores_a_selection_from_a_different_context(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        ClusterRegistry::insert_test_session(cx, "a", ConnectionState::Connecting);
    });

    let panel = cx.update(|cx| {
        cx.new(|cx| LogsPanel::new(PanelScope::new(NavTarget::Logs, "a".to_string()), cx))
    });

    cx.update(|cx| {
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "default".into(),
            name: "web-1".into(),
            containers: vec!["web".into()],
            context_name: "b".into(),
        })));
    });
    cx.run_until_parked();

    panel.read_with(cx, |panel, _| {
        assert!(
            panel.current.is_none(),
            "a selection from a different context must not be streamed here"
        );
    });
}
