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

use super::super::super::render::{FOLLOW_OFF, FOLLOW_ON, follow_tooltip};

impl Harness {
    /// Whether the Follow button is drawn in its on style, after checking it
    /// is drawn in exactly one of the two.
    fn button_on(&mut self) -> bool {
        let on = self.vcx.debug_bounds(FOLLOW_ON).is_some();
        let off = self.vcx.debug_bounds(FOLLOW_OFF).is_some();
        assert_ne!(on, off, "the Follow button shows one state");
        on
    }

    fn click_follow(&mut self) {
        let selector = if self.button_on() {
            FOLLOW_ON
        } else {
            FOLLOW_OFF
        };
        let at = self
            .vcx
            .debug_bounds(selector)
            .expect("the Follow button is drawn")
            .center();
        self.vcx.simulate_click(at, Modifiers::none());
        self.draw();
    }

    fn at_end(&mut self) -> Option<bool> {
        let panel = self.panel.clone();
        self.vcx
            .update(|_, cx| panel.read(cx).scroll_handle.is_scrolled_to_end())
    }

    fn scroll_to_newest(&mut self) -> bool {
        let panel = self.panel.clone();
        self.vcx.update(|_, cx| {
            let panel = panel.read(cx);
            panel.scroll_to_newest(panel.view.read(cx).row_count())
        })
    }
}

/// `logs-follow-controls` 1.1 and 1.3: the button's style is the follow
/// state, so scrolling up flips it off and scrolling back to the end on.
#[gpui_kit::test]
async fn the_follow_button_shows_the_follow_state(cx: &mut TestAppContext) {
    let mut h = open(cx);
    assert!(h.button_on(), "following shows the button on");

    h.wheel(10.);
    assert_eq!(h.follow(), FollowState::Paused);
    assert!(!h.button_on(), "scrolling up shows it off");

    h.wheel(-1000.);
    assert_eq!(h.follow(), FollowState::Following);
    assert!(h.button_on(), "back at the end shows it on again");
}

/// `logs-follow-controls` 1.2: the tooltip says which state Follow is in.
#[test]
fn the_follow_tooltip_names_the_state() {
    assert_eq!(follow_tooltip(true), "Following new lines - click to stop");
    assert_eq!(follow_tooltip(false), "Follow new lines as they arrive");
}

/// `logs-follow-controls` 2.1: going to the newest line is a no-op at the
/// end and a scroll to the last line anywhere else.
#[gpui_kit::test]
async fn scroll_to_newest_moves_only_when_not_at_the_end(cx: &mut TestAppContext) {
    let mut h = open(cx);
    assert_eq!(h.at_end(), Some(true));
    assert!(!h.scroll_to_newest(), "already at the end: no scroll");

    h.wheel(10.);
    assert_eq!(h.at_end(), Some(false));
    assert!(h.scroll_to_newest(), "scrolled back: it scrolls");
    h.draw();
    assert_eq!(h.at_end(), Some(true), "to the last line");
}

/// `logs-follow-controls` 2.2: turning Follow on goes to the newest line at
/// once, with no new line arriving; turning it off leaves the view alone.
#[gpui_kit::test]
async fn clicking_follow_on_goes_to_the_newest_line_at_once(cx: &mut TestAppContext) {
    let mut h = open(cx);
    h.wheel(10.);
    let reading = h.scrolled();
    assert_eq!(h.at_end(), Some(false));

    h.click_follow();
    assert_eq!(h.follow(), FollowState::Following, "the click follows");
    assert!(h.button_on());
    assert_eq!(h.at_end(), Some(true), "at the newest line");
    let newest = h.scrolled();
    assert!(
        newest > reading,
        "the view moved down: {newest} > {reading}"
    );

    h.click_follow();
    assert_eq!(h.follow(), FollowState::Paused, "a second click stops");
    assert!(!h.button_on());
    assert_eq!(h.scrolled(), newest, "stopping leaves the view where it is");
    h.append("line 301");
    assert_eq!(h.scrolled(), newest, "and new lines no longer move it");
}

/// The keyboard route: `s` (k9s's autoscroll key), bound through the command
/// registry, toggles Follow while the panel has focus - turning it on goes to
/// the newest line at once, as the click does.
#[gpui_kit::test]
async fn s_toggles_follow_from_the_keyboard(cx: &mut TestAppContext) {
    let mut h = open(cx);
    let panel = h.panel.clone();
    h.vcx.update(|window, cx| {
        let mut registry = crate::command::CommandRegistry::new();
        crate::util::logs::register_commands(&mut registry);
        let bindings = crate::keymap::bindings(
            &registry,
            &crate::keymap::KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
        window.activate_window();
        gpui_kit::Focusable::focus_handle(panel.read(cx), cx).focus(window, cx);
    });
    h.wheel(10.);
    assert_eq!(h.at_end(), Some(false));

    h.vcx.simulate_keystrokes("s");
    h.draw();
    assert_eq!(h.follow(), FollowState::Following, "s follows");
    assert!(h.button_on());
    assert_eq!(h.at_end(), Some(true), "at the newest line");

    h.vcx.simulate_keystrokes("s");
    h.draw();
    assert_eq!(h.follow(), FollowState::Paused, "s again stops");
    assert!(!h.button_on());
}
