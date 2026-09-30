// Not `use super::*`: `logs.rs` pulls in `gpui_kit::*`, whose own `test`
// attribute macro would shadow `core::prelude::v1::test` for the plain
// synchronous tests below.
use super::{FollowState, LogEvent, LogsView};

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
