//! #252: the context name sits on the search box's line, to its right,
//! truncated when long, with the same gap above the table as every list
//! panel.

use super::{deployments, harness, harness_full, nodes, object};
use crate::ui::list_search::{ContextShown, assert_search_row_layout};
use gpui_kit::TestAppContext;

#[gpui_kit::test]
async fn the_context_sits_right_of_the_search_box(cx: &mut TestAppContext) {
    let mut h = harness(cx, deployments(), vec![object("web", Some("shop"))]);
    assert_search_row_layout(&mut h.vcx, ContextShown::Whole);
}

#[gpui_kit::test]
async fn a_long_context_name_truncates(cx: &mut TestAppContext) {
    let long = format!("context-{}", "x".repeat(400));
    let mut h = harness_full(
        cx,
        deployments(),
        &long,
        vec![object("web", Some("shop"))],
        None,
        &[],
        |_| {},
    );
    assert_search_row_layout(&mut h.vcx, ContextShown::Truncated);
}

#[gpui_kit::test]
async fn a_cluster_scoped_list_keeps_the_same_row(cx: &mut TestAppContext) {
    let mut h = harness(cx, nodes(), vec![object("node-a", None)]);
    assert_search_row_layout(&mut h.vcx, ContextShown::Whole);
}

/// A list the server refuses has no search row, yet still names its context.
#[gpui_kit::test]
async fn a_refused_list_still_names_its_context(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;
    let mut h = super::harness_with(cx, deployments(), Vec::new(), Some("forbidden"));
    h.vcx.update(|window, cx| window.render_frame(cx));
    assert!(
        h.vcx
            .debug_bounds(crate::ui::list_search::SEARCH_ROW)
            .is_none()
    );
    let line = h
        .vcx
        .debug_bounds(crate::ui::list_search::CONTEXT_LINE)
        .expect("the context is named");
    assert!(line.size.width > gpui_kit::px(0.));
}
