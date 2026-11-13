//! #365: a log line wider than the panel scrolls into view sideways instead
//! of being cut off at the panel's edge.

use super::super::LogsPanel;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use crate::util::logs::LogLine;
use crate::util::logs::render::widest_row;
use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, Modifiers, TestAppContext, VisualTestContext, point, px};

const LINES: &str = super::super::super::render::LINES;

/// The list measures its widest row - a line, or the end marker when that is
/// wider - so the scrollable width fits the longest thing in it.
#[test]
fn the_widest_row_sizes_the_list() {
    let lines: Vec<LogLine> = ["short", "the longest line", "mid-length"]
        .into_iter()
        .map(|text| LogLine::plain(text.into()))
        .collect();
    assert_eq!(widest_row(&lines, None), 1);
    assert_eq!(widest_row(&lines, Some("ended")), 1);
    assert_eq!(
        widest_row(&lines, Some("the stream ended here")),
        3,
        "the marker row"
    );
    assert_eq!(widest_row(&[], None), 0);
}

#[gpui_kit::test]
async fn a_long_line_scrolls_sideways(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
    });
    let long = "x".repeat(2_000);
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| {
            let mut panel = LogsPanel::new(PanelScope::new(NavTarget::Logs, "demo".into()), cx);
            panel.test_show_lines("job-1", "main", &["short", &long, "short"], cx);
            panel
        });
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    let at = vcx
        .debug_bounds(LINES)
        .expect("the lines are drawn")
        .center();

    let offset_x = |vcx: &mut VisualTestContext| {
        vcx.update(|_, cx| {
            panel
                .read(cx)
                .scroll_handle
                .0
                .borrow()
                .base_handle
                .offset()
                .x
        })
    };
    assert_eq!(offset_x(&mut vcx), px(0.));

    vcx.simulate_event(gpui_kit::ScrollWheelEvent {
        position: at,
        delta: gpui_kit::ScrollDelta::Pixels(point(px(-300.), px(0.))),
        modifiers: Modifiers::none(),
        touch_phase: gpui_kit::TouchPhase::Moved,
    });
    vcx.run_until_parked();
    assert!(
        offset_x(&mut vcx) < px(0.),
        "a sideways scroll moved the lines left"
    );
}
