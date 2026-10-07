//! #149: scrolling back through the Logs panel stops it following, so new
//! lines don't pull the view back down while history is being read; scrolling
//! back down to the end follows again.

use super::super::LogsPanel;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use crate::util::logs::FollowState;
use gpui_kit::component::Root;
use gpui_kit::{
    AppContext as _, Entity, Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext,
    VisualTestContext, point,
};

struct Harness {
    panel: Entity<LogsPanel>,
    vcx: VisualTestContext,
}

fn open(cx: &mut TestAppContext) -> Harness {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
    });
    let lines: Vec<String> = (0..300).map(|n| format!("line {n}")).collect();
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| {
            let mut panel = LogsPanel::new(PanelScope::new(NavTarget::Logs, "demo".into()), cx);
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            panel.test_show_lines("web-1", "web", &lines, cx);
            panel
        });
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let mut harness = Harness {
        panel,
        vcx: VisualTestContext::from_window(window.into(), cx),
    };
    harness.draw();
    // A line arriving while following brings the view to the newest.
    harness.append("line 300");
    harness
}

impl Harness {
    fn draw(&mut self) {
        self.vcx.run_until_parked();
        self.vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    }

    fn append(&mut self, line: &str) {
        let panel = self.panel.clone();
        self.vcx
            .update(|_, cx| panel.update(cx, |panel, cx| panel.test_append_line(line, cx)));
        self.draw();
    }

    /// Scrolls the wheel over the lines by `lines` (up is positive).
    fn wheel(&mut self, lines: f32) {
        let over = self
            .vcx
            .debug_bounds(super::super::super::render::LINES)
            .expect("the lines are drawn")
            .center();
        self.vcx.simulate_event(ScrollWheelEvent {
            position: over,
            delta: ScrollDelta::Lines(point(0., lines)),
            modifiers: Modifiers::none(),
            ..Default::default()
        });
        self.draw();
    }

    fn follow(&mut self) -> FollowState {
        let panel = self.panel.clone();
        self.vcx
            .update(|_, cx| panel.read(cx).view.read(cx).follow_state())
    }

    /// How far down the list is scrolled, in pixels.
    fn scrolled(&mut self) -> f32 {
        let panel = self.panel.clone();
        self.vcx.update(|_, cx| {
            -f32::from(
                panel
                    .read(cx)
                    .scroll_handle
                    .0
                    .borrow()
                    .base_handle
                    .offset()
                    .y,
            )
        })
    }
}

#[gpui_kit::test]
async fn scrolling_back_stops_following_and_the_end_resumes_it(cx: &mut TestAppContext) {
    let mut h = open(cx);
    assert_eq!(h.follow(), FollowState::Following);
    let newest = h.scrolled();
    assert!(
        newest > 0.,
        "following shows the newest lines: scrolled {newest}px"
    );

    h.wheel(10.);
    assert_eq!(
        h.follow(),
        FollowState::Paused,
        "scrolling up stops following"
    );
    let reading = h.scrolled();
    assert!(
        reading < newest,
        "the view moved back: {reading} < {newest}"
    );

    for n in 301..321 {
        h.append(&format!("line {n}"));
    }
    assert_eq!(
        h.scrolled(),
        reading,
        "new lines leave the view where it is"
    );
    assert_eq!(h.follow(), FollowState::Paused);

    h.wheel(-1000.);
    assert_eq!(h.follow(), FollowState::Following, "the end follows again");
    h.append("line 321");
    assert!(h.scrolled() > reading, "and new lines bring the view along");
}
