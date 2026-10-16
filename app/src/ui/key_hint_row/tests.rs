//! The row's fit decisions over plain pixel values, then the drawn row: a
//! narrow one collapses to its essential hints and an overflow control whose
//! tooltip lists the rest, and widening it expands it again.

use super::fit::{Fit, ItemKind, Measured, laid_out_lines, packed_lines, shown};
use super::{KeyHint, KeyHintRow, control_selector, hidden_selector, row_selector};
use crate::consts::MAX_HINT_LINES;
use gpui_kit::InteractiveElement as _;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Bounds, Context, Entity, IntoElement, Keystroke, Modifiers, ParentElement as _, Pixels, Render,
    SharedString, Styled as _, TestAppContext, VisualTestContext, Window, div, point, px, size,
};

fn bounds(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
}

fn widths(list: &[f32]) -> Vec<Pixels> {
    list.iter().map(|width| px(*width)).collect()
}

#[test]
fn the_limit_is_three_lines() {
    assert_eq!(MAX_HINT_LINES, 3);
}

#[test]
fn packing_wraps_like_flexbox() {
    // 100 wide, gap 10: two 45s fit one line (45 + 10 + 45 = 100).
    assert_eq!(packed_lines(&widths(&[45., 45.]), px(10.), px(100.)), 1);
    assert_eq!(
        packed_lines(&widths(&[45., 45., 45.]), px(10.), px(100.)),
        2
    );
    // Six of them take three lines, which fits; a seventh needs a fourth.
    assert_eq!(packed_lines(&widths(&[45.; 6]), px(10.), px(100.)), 3);
    assert_eq!(packed_lines(&widths(&[45.; 7]), px(10.), px(100.)), 4);
    // An item wider than the row still gets a line.
    assert_eq!(packed_lines(&widths(&[150., 20.]), px(10.), px(100.)), 2);
    assert_eq!(packed_lines(&[], px(10.), px(100.)), 0);
}

#[test]
fn laid_out_lines_counts_rows_of_differing_heights_once() {
    // A 24px-tall button centred beside 16px hints is the same line.
    let items = [
        bounds(0., 4., 40., 16.),
        bounds(50., 0., 24., 24.),
        bounds(80., 4., 40., 16.),
        bounds(0., 28., 40., 16.),
        bounds(0., 48., 40., 16.),
    ];
    assert_eq!(laid_out_lines(&items), 3);
    assert_eq!(laid_out_lines(&items[..3]), 1);
    assert_eq!(laid_out_lines(&[]), 0);
}

#[test]
fn collapsed_shows_controls_and_essential_hints_in_order() {
    use ItemKind::{Control, EssentialHint, Hint};
    let kinds = [Control, Hint, EssentialHint, Hint, EssentialHint];
    assert_eq!(shown(&kinds, false), vec![true; 5]);
    assert_eq!(shown(&kinds, true), vec![true, false, true, false, true]);
}

#[test]
fn with_nothing_essential_a_collapsed_row_keeps_its_first_hint() {
    use ItemKind::{Control, Hint};
    assert_eq!(shown(&[Control, Hint, Hint], true), vec![true, true, false]);
}

/// `n` hints `width` wide, `gap` 10, laid out by `packed_lines`' rule in a
/// row `available` wide: their keys and bounds, the row's last.
fn layout(n: usize, width: f32, available: f32) -> (Vec<SharedString>, Vec<Bounds<Pixels>>) {
    let keys: Vec<SharedString> = (0..n).map(|i| format!("hint {i}").into()).collect();
    let (mut x, mut y) = (0., 0.);
    let mut laid = Vec::new();
    for _ in 0..n {
        if x > 0. && x + width > available {
            x = 0.;
            y += 20.;
        }
        laid.push(bounds(x, y, width, 16.));
        x += width + 10.;
    }
    laid.push(bounds(0., 0., available, y + 16.));
    (keys, laid)
}

#[test]
fn three_lines_fit_and_four_collapse() {
    let mut fit = Fit::default();
    let (keys, laid) = layout(6, 45., 100.);
    let frame = Measured {
        all: &keys,
        drawn: &keys,
        bounds: &laid,
        gap: px(10.),
    };
    assert!(!fit.measure(frame), "three lines: no change");
    assert!(!fit.collapsed);

    let (keys, laid) = layout(7, 45., 100.);
    let frame = Measured {
        all: &keys,
        drawn: &keys,
        bounds: &laid,
        gap: px(10.),
    };
    assert!(fit.measure(frame), "four lines collapse");
    assert!(fit.collapsed);
}

#[test]
fn a_collapsed_row_expands_only_once_its_measured_widths_fit() {
    let mut fit = Fit::default();
    let (keys, laid) = layout(7, 45., 100.);
    fit.measure(Measured {
        all: &keys,
        drawn: &keys,
        bounds: &laid,
        gap: px(10.),
    });
    assert!(fit.collapsed);

    // Collapsed to its first hint, in a row a little wider: seven 45s still
    // need four lines at 110, so it stays collapsed.
    let drawn = &keys[..1];
    let row = |width: f32| vec![bounds(0., 0., 45., 16.), bounds(0., 0., width, 16.)];
    let still = Measured {
        all: &keys,
        drawn,
        bounds: &row(110.),
        gap: px(10.),
    };
    assert!(!fit.measure(still));
    assert!(fit.collapsed);

    // At 160, three fit a line: seven take three lines, so it expands.
    let wide = Measured {
        all: &keys,
        drawn,
        bounds: &row(160.),
        gap: px(10.),
    };
    assert!(fit.measure(wide));
    assert!(!fit.collapsed);
}

// --- The drawn row ---

/// Ten 90px hints, "Hint 0" to "Hint 9", of which 2 and 7 are essential, in a
/// row `width` wide.
struct Row {
    width: Pixels,
}

const HINTS: usize = 10;

fn label(n: usize) -> String {
    format!("Hint {n}")
}

impl Render for Row {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let hints = (0..HINTS).map(|n| {
            let hint =
                KeyHint::styled(Kbd::new(Keystroke::parse("x").unwrap()), label(n), |body| {
                    body.debug_selector(move || label(n)).w(px(90.)).h(px(16.))
                });
            if n == 2 || n == 7 {
                hint.essential()
            } else {
                hint
            }
        });
        div().size_full().child(
            KeyHintRow::new("test-row")
                .w(self.width)
                .hint_gap(px(8.))
                .hints(hints),
        )
    }
}

fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
        vcx.run_until_parked();
    }
}

fn drawn(vcx: &mut VisualTestContext, selector: String) -> bool {
    vcx.debug_bounds(selector.leak()).is_some()
}

fn open(cx: &mut TestAppContext, width: f32) -> (Entity<Row>, &mut VisualTestContext) {
    cx.update(crate::util::test_ui::init);
    let (row, vcx) = cx.add_window_view(|_, _| Row { width: px(width) });
    settle(vcx);
    (row, vcx)
}

#[gpui_kit::test]
fn a_wide_row_draws_every_hint(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx, 2000.);
    assert!(drawn(vcx, row_selector("test-row", false)));
    assert!((0..HINTS).all(|n| drawn(vcx, label(n))));
}

#[gpui_kit::test]
fn a_narrow_row_draws_only_its_essential_hints_and_the_control(cx: &mut TestAppContext) {
    // Two 90px hints a line: ten would take five lines.
    let (_, vcx) = open(cx, 200.);
    assert!(drawn(vcx, row_selector("test-row", true)));
    let shown: Vec<usize> = (0..HINTS).filter(|n| drawn(vcx, label(*n))).collect();
    assert_eq!(shown, vec![2, 7]);
}

#[gpui_kit::test]
fn widening_a_collapsed_row_expands_it(cx: &mut TestAppContext) {
    let (row, vcx) = open(cx, 200.);
    assert!(drawn(vcx, row_selector("test-row", true)));
    // Four a line: ten take three lines.
    row.update(vcx, |row, cx| {
        row.width = px(400.);
        cx.notify();
    });
    settle(vcx);
    assert!(drawn(vcx, row_selector("test-row", false)));
    assert!((0..HINTS).all(|n| drawn(vcx, label(n))));
}

#[gpui_kit::test]
fn the_overflow_tooltip_lists_each_hidden_hint(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx, 200.);
    let at = vcx
        .debug_bounds(control_selector("test-row").leak())
        .expect("the overflow control")
        .center();
    vcx.simulate_mouse_move(at, None, Modifiers::none());
    vcx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    settle(vcx);
    assert!(drawn(
        vcx,
        crate::ui::icon_tooltip::selector(super::TOOLTIP)
    ));
    for n in (0..HINTS).filter(|n| *n != 2 && *n != 7) {
        assert!(
            drawn(vcx, hidden_selector(&label(n))),
            "{} listed",
            label(n)
        );
    }
    assert!(
        !drawn(vcx, hidden_selector(&label(2))),
        "a shown hint isn't"
    );
}
