//! Task 1.1: which group lies in a direction, on pane trees built with the
//! dock's own edits - a two-pane split, an L of one pane beside two stacked,
//! and a lone pane.

use super::{Direction, adjacent, group_rects};
use gpui_kit::EntityId;
use gpui_kit::component::Placement;
use gpui_kit::component::dock::{NodeId, PaneTree, PanelId, RootKind};

fn panel(n: u64) -> PanelId {
    PanelId::from(EntityId::from(n))
}

/// A tree holding `first` alone.
fn tree_with(first: PanelId) -> PaneTree {
    let mut tree = PaneTree::new(RootKind::Split);
    let root = tree.root().id();
    assert!(tree.split(root, first, Placement::Right, None).changed());
    tree
}

fn group(tree: &PaneTree, panel: PanelId) -> NodeId {
    tree.find_panel_node(panel)
        .expect("the panel is in the tree")
}

fn split(tree: &mut PaneTree, beside: PanelId, panel: PanelId, placement: Placement) {
    let at = group(tree, beside);
    assert!(tree.split(at, panel, placement, None).changed());
}

fn next(tree: &PaneTree, from: PanelId, direction: Direction) -> Option<NodeId> {
    adjacent(&group_rects(tree.root()), group(tree, from), direction)
}

#[test]
fn in_a_two_pane_split_each_pane_is_the_others_neighbour() {
    let (a, b) = (panel(1), panel(2));
    let mut tree = tree_with(a);
    split(&mut tree, a, b, Placement::Right);

    assert_eq!(next(&tree, a, Direction::Right), Some(group(&tree, b)));
    assert_eq!(next(&tree, b, Direction::Left), Some(group(&tree, a)));
    assert_eq!(
        next(&tree, a, Direction::Left),
        None,
        "nothing left of the left pane"
    );
    assert_eq!(next(&tree, a, Direction::Up), None);
    assert_eq!(next(&tree, b, Direction::Down), None);
}

/// `a` | (`b` over `c`): the stacked pair are each other's neighbours and both
/// have `a` to their left; from `a`, right is ambiguous - `b` and `c` are
/// equally near and equally overlapping - so there is none.
#[test]
fn an_l_shaped_layout_resolves_what_is_unambiguous() {
    let (a, b, c) = (panel(1), panel(2), panel(3));
    let mut tree = tree_with(a);
    split(&mut tree, a, b, Placement::Right);
    split(&mut tree, b, c, Placement::Bottom);

    assert_eq!(next(&tree, b, Direction::Down), Some(group(&tree, c)));
    assert_eq!(next(&tree, c, Direction::Up), Some(group(&tree, b)));
    assert_eq!(next(&tree, b, Direction::Left), Some(group(&tree, a)));
    assert_eq!(next(&tree, c, Direction::Left), Some(group(&tree, a)));
    assert_eq!(
        next(&tree, a, Direction::Right),
        None,
        "two equal candidates: no guess"
    );
    assert_eq!(next(&tree, b, Direction::Right), None);
}

#[test]
fn a_lone_pane_has_no_neighbour() {
    let a = panel(1);
    let tree = tree_with(a);
    for direction in [
        Direction::Left,
        Direction::Right,
        Direction::Up,
        Direction::Down,
    ] {
        assert_eq!(next(&tree, a, direction), None, "{direction:?}");
    }
}
