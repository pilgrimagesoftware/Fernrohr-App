//! The Pods panel's hint row in a narrow window (`key-hint-rows`): it
//! collapses to Filter and Describe plus the overflow control, the control's
//! tooltip lists the hidden hints (Port forward included, D5), a hidden hint's
//! key still runs its command. The keyboard route to the control, which needs
//! the main window's Key Hints handler, is in `real_window_tests`.

use super::super::test_window::{Harness, open};
use super::{HINTS_ID, hint_selector};
use crate::ui::key_hint_row::{TOOLTIP, control_selector, hidden_selector, row_selector};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Modifiers, TestAppContext, px, size};

/// Whether the element with debug selector `selector` is drawn.
fn drawn(h: &mut Harness, selector: String) -> bool {
    h.vcx.update(|window, cx| window.render_frame(cx));
    h.vcx.debug_bounds(selector.leak()).is_some()
}

/// The harness's panel, its window made `width` wide and redrawn until the
/// row has settled.
fn narrow(cx: &mut TestAppContext, width: f32) -> Harness {
    let mut harness = open(cx);
    harness.vcx.simulate_resize(size(px(width), px(700.)));
    for _ in 0..3 {
        harness.vcx.update(|window, cx| window.render_frame(cx));
        harness.vcx.run_until_parked();
    }
    harness
}

#[gpui_kit::test]
async fn a_wide_pods_row_shows_every_hint(cx: &mut TestAppContext) {
    let mut h = narrow(cx, 2400.);
    assert!(drawn(&mut h, row_selector(HINTS_ID, false)));
    assert!(drawn(&mut h, hint_selector("Quick look")));
    assert!(!drawn(&mut h, control_selector(HINTS_ID)));
}

#[gpui_kit::test]
async fn a_narrow_pods_row_keeps_filter_and_describe(cx: &mut TestAppContext) {
    let mut h = narrow(cx, 320.);
    assert!(drawn(&mut h, row_selector(HINTS_ID, true)));
    assert!(drawn(&mut h, hint_selector("Filter")));
    assert!(drawn(&mut h, hint_selector("Describe")));
    assert!(!drawn(&mut h, hint_selector("Quick look")));
    assert!(drawn(&mut h, control_selector(HINTS_ID)));
}

#[gpui_kit::test]
async fn the_overflow_tooltip_lists_a_hidden_port_forward(cx: &mut TestAppContext) {
    let mut h = narrow(cx, 320.);
    assert!(!drawn(&mut h, hint_selector("Port forward")), "hidden");
    let at = h
        .vcx
        .debug_bounds(control_selector(HINTS_ID).leak())
        .expect("the overflow control")
        .center();
    h.vcx.simulate_mouse_move(at, None, Modifiers::none());
    h.vcx
        .executor()
        .advance_clock(std::time::Duration::from_secs(1));
    h.vcx.run_until_parked();
    assert!(drawn(&mut h, crate::ui::icon_tooltip::selector(TOOLTIP)));
    assert!(drawn(&mut h, hidden_selector("Port forward")));
    assert!(drawn(&mut h, hidden_selector("Quick look")));
}

#[gpui_kit::test]
async fn a_hidden_hints_key_still_runs_its_command(cx: &mut TestAppContext) {
    let mut h = narrow(cx, 320.);
    assert!(!drawn(&mut h, hint_selector("Quick look")), "hidden");
    h.press("space");
    assert_eq!(h.looking_at().as_deref(), Some("web-1"));
}
