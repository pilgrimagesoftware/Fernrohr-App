// Not `use super::*`: `logs.rs` pulls in `gpui_kit::*`, whose own `test`
// attribute macro would shadow `core::prelude::v1::test` for the plain
// synchronous tests below.
use super::{ENDED_MARKER, FollowState, LogEvent, LogsView, NO_OUTPUT};
use crate::util::logs::line::LineSource;

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

/// `1-window-context-bar` bug 2: a failure carries a readable message *and*
/// its full technical detail, rather than folding the latter into the former.
/// A plain end is no longer a terminal message at all (`completed-pod-logs`).
#[test]
fn a_failure_is_a_terminal_message_and_an_end_is_not() {
    let mut stream_ended = LogsView::new(vec!["app".into()]);
    stream_ended.apply(LogEvent::Line("some log line".into()));
    stream_ended.apply(LogEvent::Ended);

    let mut request_failed = LogsView::new(vec!["app".into()]);
    request_failed.apply(LogEvent::RequestFailed {
        message: "container not started".into(),
        detail: "RequestFailed(...)".into(),
    });

    assert_eq!(stream_ended.terminal_message(), None);
    assert!(stream_ended.ended());
    let failed_message = request_failed.terminal_message().unwrap();
    assert!(
        failed_message
            .to_lowercase()
            .contains("container not started")
    );
    assert!(!request_failed.ended());
    assert_eq!(
        request_failed.terminal_detail(),
        Some("RequestFailed(...)"),
        "the full technical detail stays available alongside the readable message"
    );
}

/// `completed-pod-logs` 1.1: a stream that ends keeps every line it read,
/// with the end marker after them - a completed Job's output stays readable.
#[test]
fn an_ended_stream_keeps_its_lines_with_the_marker_after() {
    let mut view = LogsView::new(vec!["app".into()]);
    for line in ["starting", "done"] {
        view.apply(LogEvent::Line(line.into()));
    }
    view.apply(LogEvent::Ended);
    assert_eq!(view.lines(), ["starting", "done"]);
    assert_eq!(view.end_marker(), Some(ENDED_MARKER));
    assert_eq!(view.row_count(), 3, "two lines and the marker");
}

#[test]
fn an_ended_stream_with_no_lines_says_it_wrote_nothing() {
    let mut view = LogsView::new(vec!["app".into()]);
    view.apply(LogEvent::Ended);
    assert_eq!(view.end_marker(), Some(NO_OUTPUT));
    assert_eq!(view.row_count(), 1);
}

#[test]
fn a_stream_still_running_has_no_marker() {
    let mut view = LogsView::new(vec!["app".into()]);
    view.apply(LogEvent::Line("hello".into()));
    assert_eq!(view.end_marker(), None);
    assert_eq!(view.row_count(), 1);
}

/// `selector-logs-panel` 3.1: a tagged stream's line keeps its source apart
/// from its text, and counts as a row like any other line - what following
/// and jumping to the bottom go by.
#[test]
fn a_tagged_line_keeps_its_source_apart() {
    let mut view = LogsView::new(Vec::new());
    let web = LineSource::new("web-1", "web-1");
    view.apply_from(&web, LogEvent::Line("ready".into()));
    view.apply_from(&web, LogEvent::Ended);

    let [ready, ended] = view.lines() else {
        panic!("two lines: {:?}", view.lines());
    };
    assert_eq!(ready.text(), "ready");
    assert_eq!(ready.source(), Some(&web));
    assert_eq!(ended.text(), ENDED_MARKER, "one stream's end is a line");
    assert_eq!(view.row_count(), 2);
    assert_eq!(view.end_marker(), None, "and not the view's end");
}
