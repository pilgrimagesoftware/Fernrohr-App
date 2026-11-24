//! #378: drawing the panel reads the lines it shows from the view instead of
//! copying the whole history every frame, so a frame's cost follows the
//! visible rows, not how long the stream has run.

use super::super::LogsPanel;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use crate::util::logs::line::clone_count;
use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext};

const LINES: &str = super::super::super::render::LINES;

#[gpui_kit::test]
async fn drawing_a_long_history_copies_no_lines(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
    });
    let lines: Vec<String> = (0..20_000).map(|n| format!("line {n}")).collect();
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| {
            let mut panel = LogsPanel::new(PanelScope::new(NavTarget::Logs, "demo".into()), cx);
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            panel.test_show_lines("job-1", "main", &lines, cx);
            panel
        });
        Root::new(panel, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();

    let before = clone_count();
    for _ in 0..3 {
        vcx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
    }

    assert!(vcx.debug_bounds(LINES).is_some(), "the lines are drawn");
    assert_eq!(clone_count() - before, 0, "a frame copies no lines");
}
