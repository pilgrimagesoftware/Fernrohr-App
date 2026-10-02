//! `visual-refresh-typography-spacing` 3.1: the scale's values and how they
//! scale with the text size.

use super::{Spacing, TextScale, spacing};
use gpui_kit::{TestAppContext, px};

/// At the default size, every token is a step roomier than the literal it
/// replaces.
#[test]
fn the_default_scale_is_roomier_than_before() {
    let space = Spacing::at(TextScale::DEFAULT);
    assert_eq!(space.panel_inset, px(16.));
    assert_eq!(space.card_padding, px(12.));
    assert_eq!(space.row_height, px(36.));
    assert_eq!(space.section_gap, px(16.));
    assert_eq!(space.control_gap, px(10.));
    for (token, was) in [
        (space.panel_inset, 12.),
        (space.card_padding, 8.),
        (space.row_height, 32.),
        (space.section_gap, 12.),
        (space.control_gap, 8.),
    ] {
        assert!(token > px(was), "{token:?} is roomier than {was}");
    }
}

/// Every token grows in proportion to the text size, rounded to a whole pixel.
#[test]
fn tokens_scale_with_the_text_size() {
    let base = Spacing::at(TextScale::DEFAULT);
    let large = Spacing::at(TextScale::new(1.5).unwrap());
    let small = Spacing::at(TextScale::new(0.85).unwrap());
    assert_eq!(large.panel_inset, px(24.));
    assert_eq!(large.row_height, px(54.));
    assert_eq!(large.control_gap, px(15.));
    // 0.85 x 10 = 8.5 rounds up; 0.85 x 16 = 13.6 rounds to 14.
    assert_eq!(small.control_gap, px(9.));
    assert_eq!(small.panel_inset, px(14.));
    for (small, base, large) in [
        (small.panel_inset, base.panel_inset, large.panel_inset),
        (small.card_padding, base.card_padding, large.card_padding),
        (small.row_height, base.row_height, large.row_height),
        (small.section_gap, base.section_gap, large.section_gap),
        (small.control_gap, base.control_gap, large.control_gap),
    ] {
        assert!(
            small < base && base < large,
            "{small:?} < {base:?} < {large:?}"
        );
        assert_eq!(f32::from(large), f32::from(large).round(), "whole pixels");
    }
}

#[test]
fn a_scale_is_positive_and_finite() {
    assert_eq!(TextScale::new(1.2).map(TextScale::factor), Some(1.2));
    for bad in [0., -1., f32::NAN, f32::INFINITY] {
        assert_eq!(TextScale::new(bad), None, "{bad} is not a scale");
    }
}

/// Until something sets the scale, the app reads the default; setting one
/// changes the tokens every view asks for.
#[gpui_kit::test]
fn the_apps_tokens_follow_the_set_scale(cx: &mut TestAppContext) {
    cx.update(|cx| {
        assert_eq!(TextScale::current(cx), TextScale::DEFAULT);
        assert_eq!(spacing(cx), Spacing::at(TextScale::DEFAULT));

        let larger = TextScale::new(1.25).unwrap();
        larger.set(cx);
        assert_eq!(TextScale::current(cx), larger);
        assert_eq!(spacing(cx).panel_inset, px(20.));
    });
}
