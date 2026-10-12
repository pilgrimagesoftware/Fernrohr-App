//! `completed-pod-logs` 1.2-1.3: a stream that ends keeps its lines on screen
//! with the end marker after them - a completed Job's output - while a
//! failure still takes the panel over with its reason; and following an
//! ended stream scrolls to the marker.

use super::super::LogsPanel;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use crate::util::logs::LogEvent;
use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};

const LINES: &str = super::super::super::render::LINES;
const END_MARKER: &str = super::super::super::render::END_MARKER;

struct Harness {
    panel: Entity<LogsPanel>,
    vcx: VisualTestContext,
}

/// A pod's logs panel showing `lines`, still streaming.
fn open(cx: &mut TestAppContext, lines: &[&str]) -> Harness {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
    });
    let lines: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| {
            let mut panel = LogsPanel::new(PanelScope::new(NavTarget::Logs, "demo".into()), cx);
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            panel.test_show_lines("job-1", "main", &lines, cx);
            panel
        });
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let mut h = Harness {
        panel: built.expect("the window built its panel"),
        vcx: VisualTestContext::from_window(window.into(), cx),
    };
    h.draw();
    h
}

impl Harness {
    fn draw(&mut self) {
        self.vcx.run_until_parked();
        self.vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    }

    fn apply(&mut self, event: LogEvent) {
        let panel = self.panel.clone();
        self.vcx.update(|_, cx| {
            let view = panel.read(cx).view.clone();
            view.update(cx, |view, cx| {
                view.apply(event);
                cx.notify();
            });
        });
        self.draw();
    }

    fn drawn(&mut self, selector: &'static str) -> Option<gpui_kit::Bounds<gpui_kit::Pixels>> {
        self.vcx.debug_bounds(selector)
    }
}

#[gpui_kit::test]
async fn an_ended_stream_keeps_its_lines_with_the_marker_after(cx: &mut TestAppContext) {
    let mut h = open(cx, &["starting job", "processed 3 items", "done"]);
    h.apply(LogEvent::Ended);

    let lines = h.drawn(LINES).expect("the lines are still drawn");
    let marker = h.drawn(END_MARKER).expect("the end marker is drawn");
    assert!(
        lines.contains(&marker.origin),
        "the marker is a row of the list"
    );
    let lines = h.vcx.update(|_, cx| {
        let panel = h.panel.read(cx);
        panel.view.read(cx).lines().to_vec()
    });
    assert_eq!(lines, ["starting job", "processed 3 items", "done"]);
}

#[gpui_kit::test]
async fn the_marker_comes_after_the_last_line(cx: &mut TestAppContext) {
    let mut h = open(cx, &["only line"]);
    let before = h.drawn(LINES).expect("drawn");
    h.apply(LogEvent::Ended);
    let marker = h.drawn(END_MARKER).expect("the end marker is drawn");
    assert!(
        marker.origin.y > before.origin.y,
        "below the top of the list, after its line"
    );
}

#[gpui_kit::test]
async fn a_failed_stream_shows_its_reason_and_no_lines(cx: &mut TestAppContext) {
    let mut h = open(cx, &[]);
    h.apply(LogEvent::RequestFailed {
        message: "container not started".into(),
        detail: "detail".into(),
    });
    assert!(h.drawn(LINES).is_none(), "a failure takes the panel over");
    assert!(h.drawn(END_MARKER).is_none());
}

#[gpui_kit::test]
async fn following_an_ended_stream_lands_on_the_marker(cx: &mut TestAppContext) {
    let lines: Vec<String> = (0..300).map(|n| format!("line {n}")).collect();
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let mut h = open(cx, &lines);
    h.apply(LogEvent::Ended);
    // Only the rows in view are drawn: the marker is, so the list scrolled to it.
    assert!(
        h.drawn(END_MARKER).is_some(),
        "following scrolled to the marker, the last row"
    );
}

/// `logs-follow-controls` with `completed-pod-logs`: the end marker is the
/// last row, so turning Follow on after the stream ended goes to it, not to
/// the line above it.
#[gpui_kit::test]
async fn turning_follow_on_after_the_end_lands_on_the_marker(cx: &mut TestAppContext) {
    let lines: Vec<String> = (0..300).map(|n| format!("line {n}")).collect();
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let mut h = open(cx, &lines);
    let panel = h.panel.clone();
    // Paused at the top, so ending the stream leaves the view there.
    h.vcx.update(|_, cx| {
        panel.update(cx, |panel, cx| {
            panel.toggle_follow(cx);
            panel
                .scroll_handle
                .scroll_to_item(0, gpui_kit::ScrollStrategy::Top);
        })
    });
    h.draw();
    h.apply(LogEvent::Ended);
    assert!(h.drawn(END_MARKER).is_none(), "paused at the top");

    h.vcx
        .update(|_, cx| panel.update(cx, |panel, cx| panel.toggle_follow(cx)));
    h.draw();
    assert!(
        h.drawn(END_MARKER).is_some(),
        "Follow on scrolled to the marker"
    );
    let at_end = h
        .vcx
        .update(|_, cx| panel.read(cx).scroll_handle.is_scrolled_to_end());
    assert_eq!(at_end, Some(true));
}
