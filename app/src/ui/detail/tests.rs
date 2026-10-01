//! Tests for the shared detail-view pieces.

use super::striped;
use gpui_kit::{
    AvailableSpace, IntoElement, ParentElement as _, TestAppContext, div, point, px, size,
};

/// Every other row sits on the stripe fill, starting with the second.
#[gpui_kit::test]
fn rows_alternate_the_stripe(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let cx = cx.add_empty_window();
    cx.draw(
        point(px(0.), px(0.)),
        size(
            AvailableSpace::Definite(px(400.)),
            AvailableSpace::MinContent,
        ),
        |_window, cx| {
            striped(
                (0..4).map(|ix| div().child(format!("row {ix}")).into_any_element()),
                cx,
            )
        },
    );
    for (ix, fill) in [(0, "plain"), (1, "stripe"), (2, "plain"), (3, "stripe")] {
        let selector: &'static str = format!("detail-row-{ix}-{fill}").leak();
        assert!(cx.debug_bounds(selector).is_some(), "row {ix} is {fill}");
    }
}
