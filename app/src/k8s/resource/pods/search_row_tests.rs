//! #252: the Pods panel's search row matches the other list panels - its
//! context on the search box's line, and one gap above the table, where the
//! box used to sit flush against it.

use crate::k8s::resource::pods::test_window::open;
use crate::ui::list_search::assert_search_row_layout;
use gpui_kit::TestAppContext;

#[gpui_kit::test]
async fn the_pods_search_row_matches_the_other_lists(cx: &mut TestAppContext) {
    let mut h = open(cx);
    assert_search_row_layout(&mut h.vcx, true);
}
