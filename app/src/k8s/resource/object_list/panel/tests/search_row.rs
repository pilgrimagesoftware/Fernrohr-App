//! #252: the context name sits on the search box's line, to its right,
//! truncated when long, with the same gap above the table as every list
//! panel.

use super::{deployments, harness, harness_full, nodes, object};
use crate::ui::list_search::assert_search_row_layout;
use gpui_kit::TestAppContext;

#[gpui_kit::test]
async fn the_context_sits_right_of_the_search_box(cx: &mut TestAppContext) {
    let mut h = harness(cx, deployments(), vec![object("web", Some("shop"))]);
    assert_search_row_layout(&mut h.vcx, true);
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
    assert_search_row_layout(&mut h.vcx, true);
}

#[gpui_kit::test]
async fn a_cluster_scoped_list_keeps_the_same_row(cx: &mut TestAppContext) {
    let mut h = harness(cx, nodes(), vec![object("node-a", None)]);
    assert_search_row_layout(&mut h.vcx, true);
}
