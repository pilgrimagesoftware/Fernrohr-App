//! Unit tests for `ui::table_fit`'s divider hit test.

use super::HeaderBounds;
use gpui_kit::{Bounds, Edges, Pixels, point, px, size};

fn bounds_of(cells: &[(f32, f32)]) -> HeaderBounds {
    let header = HeaderBounds::default();
    *header.0.borrow_mut() = cells
        .iter()
        .map(|(left, width)| {
            Some(Bounds::new(
                point(px(*left), px(10.)),
                size(px(*width), px(20.)),
            ))
        })
        .collect();
    header
}

fn padding() -> Edges<Pixels> {
    Edges {
        top: px(4.),
        bottom: px(4.),
        left: px(8.),
        right: px(8.),
    }
}

/// Column 0's cell starts 8px before its content at 108 and is 200 wide, so
/// its divider is at 300; column 1's (content at 308, 100 wide) at 400.
#[test]
fn a_double_click_on_a_divider_names_the_column_on_its_left() {
    let header = bounds_of(&[(108., 180.), (308., 80.)]);
    let widths = [px(200.), px(100.)];
    let at = |x: f32| header.divider_at(point(px(x), px(20.)), &widths, padding());
    assert_eq!(at(300.), Some(0));
    assert_eq!(at(303.), Some(0), "within the handle's half-width");
    assert_eq!(at(398.), Some(1));
}

#[test]
fn a_double_click_off_the_dividers_or_the_header_row_names_none() {
    let header = bounds_of(&[(108., 180.), (308., 80.)]);
    let widths = [px(200.), px(100.)];
    assert_eq!(
        header.divider_at(point(px(250.), px(20.)), &widths, padding()),
        None,
        "mid-cell"
    );
    assert_eq!(
        header.divider_at(point(px(300.), px(60.)), &widths, padding()),
        None,
        "below the header row"
    );
}
