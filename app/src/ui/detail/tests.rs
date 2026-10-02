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

/// `pod-configuration-tab` 5.1: over 100 characters or more than one line is
/// large, and the preview is the first line's first 20 characters.
#[test]
fn a_value_is_large_past_a_hundred_characters_or_one_line() {
    use super::collapsible::preview;
    assert_eq!(preview("debug"), None);
    assert_eq!(
        preview(&"a".repeat(100)),
        None,
        "100 characters is not large"
    );
    assert_eq!(
        preview(&"a".repeat(101)).as_deref(),
        Some(format!("{}…", "a".repeat(20)).as_str()),
        "101 characters is large"
    );
    assert_eq!(
        preview("a\nb").as_deref(),
        Some("a…"),
        "two short lines are large"
    );
    // Characters, not bytes: 100 two-byte characters still fit.
    assert_eq!(preview(&"ü".repeat(100)), None);
    assert_eq!(
        preview(&"ü".repeat(101)).as_deref(),
        Some(format!("{}…", "ü".repeat(20)).as_str())
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

/// The detail pieces 2.2 checks, as one view: `Button` needs a view to be
/// rendering, which `VisualTestContext::draw`'s closure isn't.
struct FontSample;

impl gpui_kit::Render for FontSample {
    fn render(
        &mut self,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> impl IntoElement {
        use crate::k8s::resource::secret_value::Reveal;
        use gpui_kit::Styled as _;
        use gpui_kit::component::ActiveTheme as _;

        // The window's own default, as `Root` gives it.
        div()
            .font_family(cx.theme().font_family.clone())
            .child(super::section_heading("Metadata", cx))
            .child(super::row(
                "Labels",
                super::chips(&["app=web".to_string()], cx),
                cx,
            ))
            .child(super::row(
                "token",
                super::secret_key_row(
                    "token",
                    12,
                    None::<&Reveal>,
                    "show-token".into(),
                    "token-value".into(),
                    |_, _| {},
                    cx,
                ),
                cx,
            ))
    }
}

/// `visual-refresh-typography-spacing` 2.2: a chip and a field row are data
/// text; a section heading, and a button inside a data value, stay frame text.
#[test]
fn chips_and_rows_are_data_text_and_headings_and_buttons_frame_text() {
    use crate::ui::typography::recorder::with_recorded_text;
    use crate::ui::typography::{DATA_FAMILY, FRAME_FAMILY};
    use gpui_kit::test::TestWindowExt as _;

    with_recorded_text(|cx, recorded| {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::ui::theme::init(crate::config::ui::Theme::Light, cx);
        });
        let window: gpui_kit::AnyWindowHandle = cx.add_window(|_, _| FontSample).into();
        window
            .update(cx, |_, window, cx| window.render_frame(cx))
            .unwrap();
        assert_eq!(
            recorded.family_of("app=web").as_ref(),
            DATA_FAMILY,
            "a chip"
        );
        assert_eq!(
            recorded.family_of("Labels").as_ref(),
            DATA_FAMILY,
            "a field label"
        );
        assert_eq!(
            recorded.family_of("Metadata").as_ref(),
            FRAME_FAMILY,
            "a section heading"
        );
        assert_eq!(
            recorded.family_of("Show").as_ref(),
            FRAME_FAMILY,
            "a button in a data value"
        );
        window
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
    });
}
