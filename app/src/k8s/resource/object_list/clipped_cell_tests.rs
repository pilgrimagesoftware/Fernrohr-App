//! #390: in a kind's list, a value wider than its column shows its full text in
//! a tooltip on hover, and a value that fits has none.

use super::test_harness::{Harness, kind, open};
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Modifiers, Pixels, Point};

fn hover(harness: &mut Harness, selector: &'static str) {
    harness.vcx.update(|window, cx| window.render_frame(cx));
    let at: Point<Pixels> = harness
        .vcx
        .debug_bounds(selector)
        .expect("the cell is drawn")
        .center();
    harness.vcx.simulate_mouse_move(at, None, Modifiers::none());
    harness
        .vcx
        .executor()
        .advance_clock(std::time::Duration::from_secs(1));
    harness.vcx.run_until_parked();
}

#[gpui_kit::test]
async fn a_clipped_name_shows_in_full_on_hover(cx: &mut TestAppContext) {
    let long = "a-very-long-config-map-name-".repeat(12);
    let mut harness = open(
        cx,
        kind("", "ConfigMap", "configmaps", true),
        &["short", long.as_str()],
    );
    harness.vcx.update(|window, cx| window.render_frame(cx));
    let tooltip: &'static str = crate::ui::icon_tooltip::selector(&long).leak();

    // Rows sort by name, so the long one is first.
    hover(&mut harness, "object-cell-1-0");
    assert!(
        harness.vcx.debug_bounds(tooltip).is_none(),
        "a name that fits has no tooltip"
    );

    hover(&mut harness, "object-cell-0-0");
    assert!(
        harness.vcx.debug_bounds(tooltip).is_some(),
        "the clipped name shows in full"
    );
}
