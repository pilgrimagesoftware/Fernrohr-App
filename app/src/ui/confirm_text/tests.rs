use super::ConfirmText;
use crate::ui::typography::recorder::with_recorded_text;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{Context, IntoElement, ParentElement as _, Render, Styled as _, Window, div};

fn delete_secret() -> ConfirmText {
    ConfirmText::new()
        .text("Delete Secret ")
        .name("db-password")
        .text(" in ")
        .name("payments")
        .text("?")
}

#[test]
fn names_are_quoted_and_listed() {
    let text = delete_secret();
    assert_eq!(
        text.plain(),
        "Delete Secret \u{201c}db-password\u{201d} in \u{201c}payments\u{201d}?"
    );
    assert_eq!(text.name_list(), ["db-password", "payments"]);
}

#[test]
fn names_join_with_commas_and_append_keeps_names() {
    let text = ConfirmText::from("The tunnels for ")
        .names(&["prod", "staging"])
        .text(" will disconnect.")
        .append(ConfirmText::from(" And ").name("web-0").text("."));
    assert_eq!(
        text.plain(),
        "The tunnels for \u{201c}prod\u{201d}, \u{201c}staging\u{201d} will disconnect. \
         And \u{201c}web-0\u{201d}."
    );
    assert_eq!(text.name_list(), ["prod", "staging", "web-0"]);
}

struct Sample;

impl Render for Sample {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().w_full().child(delete_secret().render(cx))
    }
}

/// The sentence is one line whose names are runs of their own: the dialog's
/// font around them, the code font for each name - and each name in the
/// accent colour.
#[test]
fn each_name_is_its_own_run_in_the_code_font_and_accent() {
    use gpui_kit::test::TestWindowExt as _;
    with_recorded_text(|cx, recorded| {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::ui::theme::init(crate::config::ui::Theme::Light, cx);
            // The test text system has no fonts installed, so the theme's code
            // family falls back to the UI font; name one that can't be confused.
            cx.global_mut::<gpui_kit::component::Theme>()
                .mono_font_family = "Monaco".into();
        });
        let window: gpui_kit::AnyWindowHandle = cx.add_window(|_, _| Sample).into();
        window
            .update(cx, |_, window, cx| window.render_frame(cx))
            .unwrap();
        let code = cx.update(|cx| cx.theme().mono_font_family.clone());
        let families = recorded
            .families_of("db-password")
            .expect("the sentence was drawn");
        let [before, first, between, second, after] = families.as_slice() else {
            panic!("text, name, text, name, text - five runs, not {families:?}");
        };
        assert_eq!((first, second), (&code, &code), "each name is code");
        assert!(
            [before, between, after]
                .iter()
                .all(|family| **family == *before && **family != code),
            "the sentence around the names keeps the dialog's font: {families:?}"
        );
        cx.update(|cx| {
            let accent = crate::ui::style::accent(cx);
            let highlights = delete_secret().highlights(cx);
            assert_eq!(highlights.len(), 2, "one highlight per name");
            for (range, style) in highlights {
                assert_eq!(style.color, Some(accent), "{range:?} is in the accent");
            }
        });
    });
}
