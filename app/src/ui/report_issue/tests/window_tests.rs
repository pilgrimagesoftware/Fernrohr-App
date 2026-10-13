//! #241 and #249: one non-modal window, Subject focused as it opens, and the
//! type switching placeholders and the log offer but never the text.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{ElementId, Focusable as _, TestAppContext};

use super::super::body::IssueKind;
use super::super::render::LOG_CHECKBOX;
use super::super::window::{self, Route};
use super::{fill, open, setup};

#[gpui_kit::test]
async fn it_opens_as_a_bug_with_empty_fields_and_subject_focused(cx: &mut TestAppContext) {
    setup(cx);
    let mut opened = open(cx, None);
    let view = opened.view.clone();
    let (kind, subject, description, attach_log, focused) = opened.vcx.update(|window, cx| {
        let view = view.read(cx);
        (
            view.kind,
            view.subject.read(cx).value().to_string(),
            view.description.read(cx).value().to_string(),
            view.attach_log,
            view.subject.read(cx).focus_handle(cx).is_focused(window),
        )
    });
    assert_eq!(kind, IssueKind::Bug);
    assert!(subject.is_empty() && description.is_empty());
    assert!(!attach_log, "the log is opt-in");
    assert!(focused, "#249: Subject has focus");
    // No `gh` override in tests: Report goes to the browser.
    let route = opened.vcx.update(|_, cx| view.read(cx).route);
    assert_eq!(route, Route::Browser);
}

#[gpui_kit::test]
async fn report_issue_again_raises_the_open_window_unchanged(cx: &mut TestAppContext) {
    setup(cx);
    let mut opened = open(cx, None);
    fill(&mut opened, "Pods panel crashes", "Hit refresh twice.");
    let windows = cx.update(|cx| cx.windows().len());

    cx.update(|cx| window::open_or_raise(None, cx));
    cx.run_until_parked();

    assert_eq!(
        cx.update(|cx| cx.windows().len()),
        windows,
        "no second window"
    );
    let (_, view) = cx.update(window::test_view).expect("still open");
    assert_eq!(view, opened.view, "the same window");
    let subject = cx.update(|cx| view.read(cx).subject.read(cx).value().to_string());
    assert_eq!(subject, "Pods panel crashes");
}

#[gpui_kit::test]
async fn switching_type_keeps_the_text_and_only_a_bug_offers_the_log(cx: &mut TestAppContext) {
    setup(cx);
    let mut opened = open(cx, None);
    fill(
        &mut opened,
        "Group panels by namespace",
        "So I can compare them.",
    );
    let view = opened.view.clone();
    let log_shown = |opened: &mut super::Opened| {
        opened.vcx.update(|window, cx| {
            window.render_frame(cx);
            window
                .try_find(ElementId::Name(LOG_CHECKBOX.into()))
                .is_some()
        })
    };
    assert!(log_shown(&mut opened), "a bug offers the log");

    opened.vcx.update(|window, cx| {
        view.update(cx, |view, cx| view.set_kind(IssueKind::Feature, window, cx))
    });
    opened.vcx.run_until_parked();

    let (subject, description) = opened.vcx.update(|_, cx| {
        let view = view.read(cx);
        (
            view.subject.read(cx).value().to_string(),
            view.description.read(cx).value().to_string(),
        )
    });
    assert_eq!(subject, "Group panels by namespace");
    assert_eq!(description, "So I can compare them.");
    assert!(!log_shown(&mut opened), "a feature request doesn't");
}

#[gpui_kit::test]
async fn a_screenshot_can_be_removed(cx: &mut TestAppContext) {
    setup(cx);
    let mut opened = open(cx, None);
    let view = opened.view.clone();
    opened.vcx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.add_screenshots(vec!["/s/one.png".into(), "/s/two.png".into()], cx);
            view.remove_screenshot(std::path::Path::new("/s/one.png"), cx);
        })
    });
    let shots = opened.vcx.update(|_, cx| view.read(cx).screenshots.clone());
    assert_eq!(shots, vec![std::path::PathBuf::from("/s/two.png")]);
}
