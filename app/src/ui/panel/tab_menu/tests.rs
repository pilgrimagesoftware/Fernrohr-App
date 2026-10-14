//! `panel-tab-context-menu` 3.1: the tab menu's items, in order, and which are
//! disabled for a group's shape - the only tab, the rightmost, pinned or not.

use super::{TabMenuItem, items};

fn shown(pinned: &[bool], ix: usize) -> Vec<(&'static str, bool)> {
    items(pinned, ix)
        .into_iter()
        .map(|(item, disabled)| (item.label(), disabled))
        .collect()
}

#[test]
fn the_menu_lists_every_item_in_order() {
    assert_eq!(
        shown(&[false, false, false], 1),
        [
            ("Close Tab", false),
            ("Close Other Tabs", false),
            ("Close Tabs to the Right", false),
            ("Close All Tabs", false),
            ("Move to New Window", false),
            ("Pin Tab", false),
        ]
    );
}

#[test]
fn the_only_tab_disables_the_bulk_closes() {
    let menu = shown(&[false], 0);
    assert_eq!(menu[1], ("Close Other Tabs", true));
    assert_eq!(menu[2], ("Close Tabs to the Right", true));
    assert_eq!(menu[3], ("Close All Tabs", true));
}

#[test]
fn the_rightmost_tab_disables_close_to_the_right() {
    let menu = shown(&[false, false], 1);
    assert_eq!(menu[2], ("Close Tabs to the Right", true));
    assert_eq!(menu[1], ("Close Other Tabs", false));
}

#[test]
fn a_pinned_tab_offers_unpin_and_pinned_neighbours_dont_count() {
    let menu = shown(&[true, false], 0);
    assert_eq!(menu[5], ("Unpin Tab", false));
    assert_eq!(menu[1], ("Close Other Tabs", false), "one unpinned other");
    // The only other tab is pinned: nothing for the bulk closes to close.
    let menu = shown(&[true, false], 1);
    assert_eq!(menu[1], ("Close Other Tabs", true));
    assert_eq!(menu[3], ("Close All Tabs", true));
    assert_eq!(items(&[true, false], 1)[5].0, TabMenuItem::Pin);
}
