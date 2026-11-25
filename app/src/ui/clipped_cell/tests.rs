use super::is_clipped;
use gpui_kit::px;

#[test]
fn text_that_fits_is_not_clipped() {
    assert!(!is_clipped(px(80.), px(120.), px(16.), px(0.)));
}

#[test]
fn text_exactly_filling_the_room_is_not_clipped() {
    assert!(!is_clipped(px(104.), px(120.), px(16.), px(0.)));
}

#[test]
fn text_wider_than_the_room_is_clipped() {
    assert!(is_clipped(px(105.), px(120.), px(16.), px(0.)));
}

#[test]
fn an_adornment_takes_room_from_the_text() {
    // 80px of text fits a 120px column, but not beside a 24px checkbox with
    // 16px of padding.
    assert!(!is_clipped(px(80.), px(120.), px(16.), px(0.)));
    assert!(is_clipped(px(90.), px(120.), px(16.), px(24.)));
}
