//! The events browser's hint row (`key-hint-rows` 2.2): a narrow window
//! collapses it to Open object and Search plus the overflow control, a wide
//! one shows every hint.

use super::*;
use crate::ui::key_hint_row::{control_selector, row_selector};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{px, size};

const HINTS_ID: &str = super::super::render::HINTS_ID;

fn drawn(h: &mut Harness, selector: String) -> bool {
    h.vcx.update(|window, cx| window.render_frame(cx));
    h.vcx.debug_bounds(selector.leak()).is_some()
}

fn resized(cx: &mut TestAppContext, width: f32) -> Harness {
    let mut h = harness(cx, fixture());
    h.vcx.simulate_resize(size(px(width), px(700.)));
    for _ in 0..3 {
        h.vcx.update(|window, cx| window.render_frame(cx));
        h.vcx.run_until_parked();
    }
    h
}

#[gpui_kit::test]
async fn a_narrow_events_row_collapses(cx: &mut TestAppContext) {
    let mut h = resized(cx, 200.);
    assert!(drawn(&mut h, row_selector(HINTS_ID, true)));
    assert!(drawn(&mut h, control_selector(HINTS_ID)));
    assert!(!drawn(&mut h, row_selector(HINTS_ID, false)));
}

#[gpui_kit::test]
async fn a_wide_events_row_shows_every_hint(cx: &mut TestAppContext) {
    let mut h = resized(cx, 2400.);
    assert!(drawn(&mut h, row_selector(HINTS_ID, false)));
    assert!(!drawn(&mut h, control_selector(HINTS_ID)));
}
