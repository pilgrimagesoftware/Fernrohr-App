//! The window from the keyboard, through the real keymap: Escape closes it
//! and sends nothing, and ⌘↵ reports - from Description too, where Enter is
//! the text area's own.

use gpui_kit::TestAppContext;

use super::{fill, is_open, open, setup};

#[gpui_kit::test]
async fn escape_closes_the_window_and_sends_nothing(cx: &mut TestAppContext) {
    setup(cx);
    let mut opened = open(cx, None);
    fill(&mut opened, "Pods panel crashes", "Hit refresh twice.");

    opened.vcx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert!(!is_open(cx), "Escape closed it");
    assert_eq!(cx.opened_url(), None, "nothing was sent");
}

#[gpui_kit::test]
async fn secondary_enter_in_description_reports(cx: &mut TestAppContext) {
    setup(cx);
    let mut opened = open(cx, None);
    fill(&mut opened, "Pods panel crashes", "Hit refresh twice.");
    let view = opened.view.clone();
    opened.vcx.update(|window, cx| {
        let description = view.read(cx).description.clone();
        description.update(cx, |input, cx| input.focus(window, cx));
    });

    opened.vcx.simulate_keystrokes("secondary-enter");
    cx.run_until_parked();

    let url = cx.opened_url().expect("Report opened the browser");
    assert!(url.contains("title=Pods%20panel%20crashes"), "{url}");
    assert!(
        !is_open(cx),
        "the window closed once the report was handed off"
    );
}

#[gpui_kit::test]
async fn report_does_nothing_until_both_fields_are_filled(cx: &mut TestAppContext) {
    setup(cx);
    let mut opened = open(cx, None);
    fill(&mut opened, "Only a subject", "  ");

    opened.vcx.simulate_keystrokes("secondary-enter");
    cx.run_until_parked();

    assert_eq!(cx.opened_url(), None);
    assert!(is_open(cx));
}
