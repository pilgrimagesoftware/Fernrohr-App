//! How a detail header shares its row: the title's full name shows while the
//! hints can wrap to make room, and gives way - ellipsizing - only when the row
//! is too narrow for the hints' floor beside it. Narrower than both floors, the
//! title stacks above the hints and nothing passes the row's edge.

use super::{HeaderRow, row_selector};
use crate::ui::key_hint_row::{KeyHint, KeyHintRow};
use crate::ui::panel::title::item_heading;
use gpui_kit::InteractiveElement as _;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Bounds, Context, IntoElement, Keystroke, ParentElement as _, Pixels, Render, Styled as _,
    TestAppContext, Window, div, px,
};

const NAME: &str = "example-connect-deployment-7f9c8d6b5-abcde";

/// A header `width` wide: the item heading for [`NAME`], and ten 90px hints.
struct Header {
    width: Pixels,
}

impl Render for Header {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // All essential, so a row narrow enough to collapse still draws them:
        // this is about the header's layout, not the collapse.
        let hints = (0..10).map(|n| {
            KeyHint::styled(
                Kbd::new(Keystroke::parse("x").unwrap()),
                format!("{n}"),
                |body| {
                    body.debug_selector(move || format!("hint-{n}"))
                        .w(px(90.))
                        .h(px(16.))
                },
            )
            .essential()
        });
        div().w(self.width).flex().child(
            HeaderRow::new(
                "test-header",
                item_heading(NAME.to_string(), None, gpui_kit::black()),
                KeyHintRow::new("test-hints").hint_gap(px(8.)).hints(hints),
            )
            .gap(px(8.)),
        )
    }
}

/// A header drawn `width` wide: its name's and hints' bounds, and whether the
/// row stacked them.
struct Drawn {
    name: Bounds<Pixels>,
    hints: Vec<Bounds<Pixels>>,
    stacked: bool,
}

fn drawn(cx: &mut TestAppContext, width: Pixels) -> Drawn {
    cx.update(crate::util::test_ui::init);
    let (_view, cx) = cx.add_window_view(|_, _| Header { width });
    // The first frame measures the row; the second draws the layout it chose.
    cx.update(|window, cx| window.render_frame(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    let name = cx
        .debug_bounds("item-heading-name")
        .expect("the name is drawn");
    let hints = (0..10)
        .map(|n| {
            let selector: &'static str = format!("hint-{n}").leak();
            cx.debug_bounds(selector).expect("the hint is drawn")
        })
        .collect();
    let stacked: &'static str = row_selector("test-header", true).leak();
    let stacked = cx.debug_bounds(stacked).is_some();
    Drawn {
        name,
        hints,
        stacked,
    }
}

/// With room for the name and a narrow hints column, the name shows whole and
/// the hints wrap beneath one another to make room.
#[gpui_kit::test]
fn the_title_pushes_the_hints_over_and_they_wrap(cx: &mut TestAppContext) {
    let whole = drawn(cx, px(2000.)).name;
    let Drawn { name, hints, .. } = drawn(cx, px(900.));
    assert_eq!(
        name.size.width, whole.size.width,
        "the name isn't cut at 900px"
    );
    let rows: std::collections::BTreeSet<i32> = hints
        .iter()
        .map(|hint| f32::from(hint.top()) as i32)
        .collect();
    assert!(rows.len() > 1, "the hints wrap onto more rows: {hints:?}");
    assert!(
        hints.iter().all(|hint| hint.left() >= name.right()),
        "and stay beside the name"
    );
}

/// Too narrow for both, the name gives way - but no further than its floor -
/// and the hints keep theirs.
#[gpui_kit::test]
fn the_title_gives_way_only_when_tight(cx: &mut TestAppContext) {
    let whole = drawn(cx, px(2000.)).name;
    let Drawn { name, hints, .. } = drawn(cx, px(420.));
    assert!(name.size.width < whole.size.width, "the name ellipsizes");
    assert!(
        name.size.width >= px(8. * 16.) - px(1.),
        "down to its floor: {name:?}"
    );
    let left = hints
        .iter()
        .map(|hint| hint.left())
        .fold(px(f32::MAX), Pixels::min);
    assert!(left >= name.right(), "the hints don't overlap it");
}

/// Too narrow for both floors side by side, the name stacks above the hints,
/// and neither passes the row's edge.
#[gpui_kit::test]
fn a_row_too_narrow_for_both_stacks_them(cx: &mut TestAppContext) {
    let width = px(300.);
    let Drawn {
        name,
        hints,
        stacked,
    } = drawn(cx, width);
    assert!(stacked, "the row is stacked");
    assert!(
        name.right() <= width,
        "the name ellipsizes at the edge: {name:?}"
    );
    assert!(
        hints.iter().all(|hint| hint.right() <= width),
        "no hint passes the edge: {hints:?}"
    );
    assert!(
        hints.iter().all(|hint| hint.top() >= name.bottom()),
        "the hints are beneath the name"
    );
}

/// Wide enough for both floors, the row stays side by side.
#[gpui_kit::test]
fn a_row_wide_enough_for_both_floors_does_not_stack(cx: &mut TestAppContext) {
    assert!(!drawn(cx, px(420.)).stacked, "the row is side by side");
}
