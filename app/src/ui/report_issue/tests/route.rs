//! #152: Report files the issue with `gh` when it's ready - faked here by a
//! script on a scratch `PATH` - labelled for its type, and opens the browser
//! when `gh` isn't ready, or fails.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{ElementId, TestAppContext};

use super::super::body::IssueKind;
use super::super::body::LogSection;
use super::super::gh::REPO;
use super::super::gh::tests::fake_gh;
use super::super::render::LOG_PREVIEW;
use super::super::window::Route;
use super::super::{GhPathOverride, LogPathOverride};
use super::{fill, is_open, open, setup, wait_until};

/// A fake `gh` that is signed in and records each `issue create`'s arguments
/// and body.
fn recording_gh(cx: &mut TestAppContext) -> std::path::PathBuf {
    let (dir, path) = fake_gh(|dir| {
        let dir = dir.display();
        format!(
            "case \"$1\" in auth) exit 0;; esac\n\
             echo \"$@\" > '{dir}/args'\ncat > '{dir}/body'\n\
             echo 'https://github.com/{REPO}/issues/170'"
        )
    });
    cx.update(|cx| cx.set_global(GhPathOverride(path)));
    dir
}

fn wait_for_route(cx: &mut TestAppContext, opened: &super::Opened, route: Route) {
    let view = opened.view.clone();
    wait_until(cx, |cx| cx.update(|cx| view.read(cx).route) == route);
}

#[gpui_kit::test]
async fn with_gh_ready_a_bug_is_filed_labelled_with_the_log_it_showed(cx: &mut TestAppContext) {
    setup(cx);
    let dir = recording_gh(cx);
    let log = dir.join("fernrohr.log");
    std::fs::write(&log, "INFO starting\nWARN watch restarted\n").unwrap();
    cx.update(|cx| cx.set_global(LogPathOverride(log)));
    let mut opened = open(cx, None);
    wait_for_route(cx, &opened, Route::Gh);
    fill(&mut opened, "Pods panel crashes", "Hit refresh twice.");
    let view = opened.view.clone();
    opened
        .vcx
        .update(|_, cx| view.update(cx, |view, cx| view.set_attach_log(true, cx)));
    wait_until(cx, |cx| cx.update(|cx| view.read(cx).log_preview.is_some()));

    // What is shown is what will be attached.
    let shown = cx.update(|cx| view.read(cx).log_preview.clone());
    assert_eq!(
        shown,
        Some(LogSection::Tail(
            "INFO starting\nWARN watch restarted\n".into()
        ))
    );
    let preview_drawn = opened.vcx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(ElementId::Name(LOG_PREVIEW.into()))
            .is_some()
    });
    assert!(preview_drawn, "the window shows the log it attaches");

    opened
        .vcx
        .update(|window, cx| view.update(cx, |view, cx| view.send(window, cx)));
    wait_until(cx, |_| {
        dir.join("args").exists() && dir.join("body").exists()
    });
    wait_until(cx, |cx| !is_open(cx));

    let args = std::fs::read_to_string(dir.join("args")).unwrap();
    assert!(args.contains("--title Pods panel crashes"), "{args}");
    assert!(args.trim_end().ends_with("--label bug"), "{args}");
    let body = std::fs::read_to_string(dir.join("body")).unwrap();
    assert!(body.starts_with("Hit refresh twice."), "{body}");
    assert!(body.contains("<details>"), "{body}");
    assert!(
        body.contains("WARN watch restarted"),
        "the log it showed: {body}"
    );
    assert_eq!(
        cx.opened_url(),
        None,
        "no browser, and no screenshots to drop"
    );
}

#[gpui_kit::test]
async fn a_feature_request_is_labelled_enhancement_and_carries_no_log(cx: &mut TestAppContext) {
    setup(cx);
    let dir = recording_gh(cx);
    let mut opened = open(cx, None);
    wait_for_route(cx, &opened, Route::Gh);
    fill(&mut opened, "Group by namespace", "To compare them.");
    let view = opened.view.clone();
    opened.vcx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_attach_log(true, cx);
            view.set_kind(IssueKind::Feature, window, cx);
            view.send(window, cx);
        })
    });
    wait_until(cx, |_| dir.join("body").exists());
    wait_until(cx, |cx| !is_open(cx));

    let args = std::fs::read_to_string(dir.join("args")).unwrap();
    assert!(args.trim_end().ends_with("--label enhancement"), "{args}");
    let body = std::fs::read_to_string(dir.join("body")).unwrap();
    assert!(!body.contains("### Logs"), "{body}");
}

#[gpui_kit::test]
async fn when_gh_fails_report_opens_the_labelled_browser_page(cx: &mut TestAppContext) {
    setup(cx);
    let (_, path) = fake_gh(|_| {
        "case \"$1\" in auth) exit 0;; esac\ncat > /dev/null\necho 'HTTP 401' >&2\nexit 1".into()
    });
    cx.update(|cx| cx.set_global(GhPathOverride(path)));
    let mut opened = open(cx, None);
    wait_for_route(cx, &opened, Route::Gh);
    fill(&mut opened, "Pods panel crashes", "Hit refresh twice.");
    let view = opened.view.clone();
    opened.vcx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_attach_log(true, cx);
            view.send(window, cx);
        })
    });
    wait_until(cx, |cx| cx.opened_url().is_some());

    let url = cx.opened_url().unwrap_or_default();
    assert!(url.contains("title=Pods%20panel%20crashes"), "{url}");
    assert!(url.ends_with("&labels=bug"), "{url}");
    assert!(
        url.contains("Please%20attach"),
        "the log by path, not its text: {url}"
    );
}

#[gpui_kit::test]
async fn without_gh_report_opens_the_browser_and_closes(cx: &mut TestAppContext) {
    setup(cx);
    let mut opened = open(cx, None);
    fill(&mut opened, "Pods panel crashes", "Hit refresh twice.");
    let view = opened.view.clone();
    opened
        .vcx
        .update(|window, cx| view.update(cx, |view, cx| view.send(window, cx)));
    cx.run_until_parked();

    let url = cx.opened_url().expect("the browser opened");
    assert!(url.contains("title=Pods%20panel%20crashes"), "{url}");
    assert!(!is_open(cx));
}
