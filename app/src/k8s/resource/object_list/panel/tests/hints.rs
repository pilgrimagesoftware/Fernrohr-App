//! The object list's hint row (`key-hint-rows` 2.2): it wraps, so a narrow
//! window collapses it to Filter, Open and Describe plus the overflow
//! control, and a wide one shows every hint.

use super::{deployments, harness};
use crate::k8s::resource::object_list::render::HINTS_ID;
use crate::ui::key_hint_row::{control_selector, row_selector};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, px, size};

fn drawn(h: &mut super::Harness, selector: String) -> bool {
    h.vcx.update(|window, cx| window.render_frame(cx));
    h.vcx.debug_bounds(selector.leak()).is_some()
}

fn resized(cx: &mut TestAppContext, width: f32) -> super::Harness {
    let mut h = harness(cx, deployments(), vec![super::object("web", Some("shop"))]);
    h.vcx.simulate_resize(size(px(width), px(700.)));
    for _ in 0..3 {
        h.vcx.update(|window, cx| window.render_frame(cx));
        h.vcx.run_until_parked();
    }
    h
}

#[gpui_kit::test]
async fn a_narrow_object_list_collapses_its_hints(cx: &mut TestAppContext) {
    let mut h = resized(cx, 300.);
    assert!(drawn(&mut h, row_selector(HINTS_ID, true)));
    assert!(drawn(&mut h, control_selector(HINTS_ID)));
    assert!(!drawn(&mut h, row_selector(HINTS_ID, false)));
}

#[gpui_kit::test]
async fn a_wide_object_list_shows_every_hint(cx: &mut TestAppContext) {
    let mut h = resized(cx, 2400.);
    assert!(drawn(&mut h, row_selector(HINTS_ID, false)));
    assert!(!drawn(&mut h, control_selector(HINTS_ID)));
}
