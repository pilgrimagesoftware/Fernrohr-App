use super::{
    ISSUE_URL_BASE, MenuSlot, REPORT_ISSUE_COMMAND_ID, ReportIssue, build_github_issue_url,
    fields_filled, issue_body, register_commands, register_handler,
};

mod prefill;
mod route;
use crate::command::CommandRegistry;
use gpui_kit::AppContext as _;
use gpui_kit::component::WindowExt as _;

#[test]
fn the_url_targets_the_app_repos_new_issue_page() {
    let url = build_github_issue_url(
        "Pods panel crashes on refresh",
        &issue_body(
            "Steps: open a cluster, hit refresh twice.",
            "1.0.0",
            "2026-10-01, abc1234",
            "macos",
        ),
    );
    assert!(url.starts_with(ISSUE_URL_BASE));
}

#[test]
fn the_url_is_percent_encoded_and_carries_the_subject_description_and_build_fields() {
    let url = build_github_issue_url(
        "Pods panel crashes on refresh",
        &issue_body(
            "Steps: open a cluster, hit refresh twice.",
            "1.0.0",
            "2026-10-01, abc1234",
            "macos",
        ),
    );
    assert!(
        url.contains("title=Pods%20panel"),
        "the subject is missing: {url}"
    );
    assert!(
        url.contains("Steps%3A%20open"),
        "the description is missing: {url}"
    );
    assert!(url.contains("1%2E0%2E0"), "the version is missing: {url}");
    assert!(url.contains("2026%2D10%2D01"), "the date is missing: {url}");
    assert!(url.contains("abc1234"), "the commit is missing: {url}");
    assert!(url.contains("macos"), "the platform is missing: {url}");
    assert!(!url.contains('\n'), "a raw newline breaks the query string");
    assert!(!url.contains(' '), "a raw space breaks the query string");
}

#[test]
fn report_issue_is_registered_for_the_help_menu() {
    let mut registry = CommandRegistry::new();
    register_commands(&mut registry);
    let command = registry
        .get(REPORT_ISSUE_COMMAND_ID)
        .expect("report_issue is registered");
    assert_eq!(command.title, "Report Issue");
    assert_eq!(command.menu, Some(MenuSlot::Help));
}

/// Knot's `add-bug-reporting` proposal's enablement rule: Report is
/// reachable only once both fields carry more than whitespace.
#[test]
fn report_is_disabled_until_both_fields_are_non_whitespace() {
    assert!(!fields_filled("", ""));
    assert!(!fields_filled("   ", "a description"));
    assert!(!fields_filled("a subject", "   "));
    assert!(!fields_filled("", "a description"));
    assert!(fields_filled("a subject", "a description"));
}

/// A blank content view: just enough to open a real window through
/// `gpui_kit::open_window` for `WindowExt`'s dialog methods to act on.
struct Blank;

impl gpui_kit::Render for Blank {
    fn render(
        &mut self,
        _window: &mut gpui_kit::Window,
        _cx: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        gpui_kit::div()
    }
}

fn open_test_window(cx: &mut gpui_kit::TestAppContext) -> gpui_kit::AnyWindowHandle {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        let (handle, _view) =
            gpui_kit::open_window(gpui_kit::WindowOptions::default(), cx, |_, cx| {
                cx.new(|_| Blank)
            })
            .expect("test window opens");
        handle
    })
}

#[gpui_kit::test]
fn report_issue_action_opens_a_dialog(cx: &mut gpui_kit::TestAppContext) {
    cx.executor().allow_parking();
    cx.update(register_handler);
    let window = open_test_window(cx);
    window
        .update(cx, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();

    let dialog_open_before = window
        .update(cx, |_, window, cx| window.has_active_dialog(cx))
        .unwrap();
    assert!(!dialog_open_before);

    cx.update(|cx| cx.dispatch_action(&ReportIssue));
    cx.run_until_parked();

    let dialog_open_after = window
        .update(cx, |_, window, cx| window.has_active_dialog(cx))
        .unwrap();
    assert!(dialog_open_after, "ReportIssue opens a dialog");

    window
        .update(cx, |_, window, cx| window.close_all_dialogs(cx))
        .unwrap();
    window
        .update(cx, |_, window, _cx| window.remove_window())
        .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn escape_closes_the_dialog(cx: &mut gpui_kit::TestAppContext) {
    cx.executor().allow_parking();
    cx.update(register_handler);
    let window = open_test_window(cx);
    window
        .update(cx, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();

    cx.update(|cx| cx.dispatch_action(&ReportIssue));
    cx.run_until_parked();

    let dialog_open_before_escape = window
        .update(cx, |_, window, cx| window.has_active_dialog(cx))
        .unwrap();
    assert!(dialog_open_before_escape, "ReportIssue opens a dialog");

    let mut vcx = gpui_kit::VisualTestContext::from_window(window, cx);
    vcx.run_until_parked();
    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();

    let dialog_open = window
        .update(cx, |_, window, cx| window.has_active_dialog(cx))
        .unwrap();
    assert!(!dialog_open, "escape closed the dialog");

    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
}
