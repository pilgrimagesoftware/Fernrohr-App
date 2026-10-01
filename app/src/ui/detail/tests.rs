//! Tests for the shared detail-view pieces.

use super::striped;
use gpui_kit::{
    AvailableSpace, IntoElement, ParentElement as _, TestAppContext, div, point, px, size,
};

/// Every other row sits on the stripe fill, starting with the second.
#[gpui_kit::test]
fn rows_alternate_the_stripe(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let cx = cx.add_empty_window();
    cx.draw(
        point(px(0.), px(0.)),
        size(
            AvailableSpace::Definite(px(400.)),
            AvailableSpace::MinContent,
        ),
        |_window, cx| {
            striped(
                (0..4).map(|ix| div().child(format!("row {ix}")).into_any_element()),
                cx,
            )
        },
    );
    for (ix, fill) in [(0, "plain"), (1, "stripe"), (2, "plain"), (3, "stripe")] {
        let selector: &'static str = format!("detail-row-{ix}-{fill}").leak();
        assert!(cx.debug_bounds(selector).is_some(), "row {ix} is {fill}");
    }
}

/// `pod-configuration-tab` 5.1: over 20 characters or more than one line is
/// large, and the preview is the first line's first 20 characters.
#[test]
fn a_value_is_large_past_twenty_characters_or_one_line() {
    use super::collapsible::preview;
    assert_eq!(preview("debug"), None);
    assert_eq!(preview("exactly-twenty-chars"), None);
    assert_eq!(
        preview("one-over-twenty-chars").as_deref(),
        Some("one-over-twenty-char…")
    );
    assert_eq!(preview("a\nb").as_deref(), Some("a…"));
    assert_eq!(
        preview("ünïcödé-ünïcödé-ünïcödé").as_deref(),
        Some("ünïcödé-ünïcödé-ünïc…")
    );
    // A trailing newline doesn't make a second line.
    assert_eq!(preview("debug\n"), None);
}

/// `pod-configuration-tab` 5.2: a revealed Secret value is shown in full,
/// however long - Secret values never collapse.
#[test]
fn a_revealed_secret_value_is_shown_in_full() {
    use crate::k8s::resource::secret_value::{Reveal, SecretValue};
    let long = "line-one-is-well-over-twenty-characters\nline-two";
    let text = super::revealed_text(&Reveal::Shown(SecretValue::new(long.as_bytes().to_vec())));
    assert_eq!(text, long);
    assert!(!text.contains('…'));
}
