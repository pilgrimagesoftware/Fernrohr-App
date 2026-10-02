//! The "Report Issue" Help-menu command: a dialog for a subject and
//! description, shown alongside the diagnostics a bug report needs (this
//! build's version, build identifier, and platform - the same fields the
//! About window shows, so the two can never drift apart). Reporting opens the
//! browser on a prefilled GitHub issue; nothing is sent automatically.
//!
//! Modeled on `app/src/util/shell/tunnel_dialog.rs`'s `WindowExt`
//! `open_dialog`/`close_dialog` wiring. Scoped to the dialog UI only - see
//! Knot's own (unimplemented) `add-bug-reporting` proposal for a `gh issue
//! create` shell-out path this does not add.

use crate::command::{Command, CommandRegistry, MenuSlot};
use crate::consts::APP_NAME;
use crate::ui::about_window::{build_identifier, version};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, WindowExt as _};
use gpui_kit::*;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

actions!(report_issue, [ReportIssue]);

pub const REPORT_ISSUE_COMMAND_ID: &str = "help.report_issue";
pub const REPORT_ISSUE_DEFAULT_BINDING: &str = "cmd-shift-/";

const ISSUE_URL_BASE: &str = "https://github.com/pilgrimagesoftware/Fernrohr-App/issues/new";

/// `help.report_issue`: a Help-menu item, palette entry, and a default
/// keybinding - all three dispatch the same action.
pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: REPORT_ISSUE_COMMAND_ID,
        title: "Report Issue",
        default_binding: REPORT_ISSUE_DEFAULT_BINDING,
        context: None,
        action: Box::new(ReportIssue),
        menu: Some(MenuSlot::Help),
    });
}

/// Opens the dialog on activation, over whichever window is active. App-wide,
/// like the other Help items, so this needs `App::active_window` rather than
/// a window-scoped `on_action` - there is no single window it belongs to.
pub fn register_handler(cx: &mut App) {
    cx.on_action(|_: &ReportIssue, cx: &mut App| {
        // Deferred: action dispatch itself runs inside an update of the
        // active window (so a window-scoped handler can intercept first), so
        // updating that same window here - to open the dialog - would be a
        // reentrant update of a window already taken out of its slot, which
        // fails silently ("window not found") without ever running our
        // closure. Deferring runs this after that update completes and the
        // window is back in its slot.
        cx.defer(open_report_issue_dialog);
    });
}

fn open_report_issue_dialog(cx: &mut App) {
    let Some(handle) = cx.active_window() else {
        return;
    };
    let _ = handle.update(cx, |_, window, cx| {
        let subject = cx.new(|cx| InputState::new(window, cx).placeholder("Subject"));
        let description = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Steps to reproduce, and what you expected instead.")
        });
        let version = version().to_string();
        let build = build_identifier();
        let platform = std::env::consts::OS;

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let subject = subject.clone();
            let description = description.clone();
            let version = version.clone();
            let build = build.clone();
            dialog
                .title("Report Issue")
                .w(px(440.))
                .content(move |content, _window, cx| {
                    content.child(report_issue_form(
                        subject.clone(),
                        description.clone(),
                        version.clone(),
                        build.clone(),
                        platform,
                        cx,
                    ))
                })
        });
    });
}

/// Section rule from Knot's `add-bug-reporting` proposal: Report is reachable
/// only once both fields carry more than whitespace.
fn fields_filled(subject: &str, description: &str) -> bool {
    !subject.trim().is_empty() && !description.trim().is_empty()
}

fn report_issue_form(
    subject: Entity<InputState>,
    description: Entity<TextareaState>,
    version: String,
    build: String,
    platform: &'static str,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme().clone();
    let enabled = fields_filled(&subject.read(cx).value(), &description.read(cx).value());

    let label = |text: &'static str| {
        div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(text)
    };
    let diagnostic_line = |text: String| {
        div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(text)
    };

    let click_subject = subject.clone();
    let click_description = description.clone();

    div()
        .flex()
        .flex_col()
        .gap(crate::ui::space::spacing(cx).control_gap)
        .child(label("Subject"))
        .child(Input::new(&subject).w_full())
        .child(label("Description"))
        .child(Textarea::new(&description).w_full().h(px(120.)))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .rounded(px(6.))
                .bg(theme.muted)
                .child(diagnostic_line(format!("Version {version}")))
                .child(diagnostic_line(format!("Build {build}")))
                .child(diagnostic_line(format!("Platform {platform}"))),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("report-issue-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                )
                .child(
                    Button::new("report-issue-report")
                        .label("Report")
                        .primary()
                        .disabled(!enabled)
                        .on_click(move |_, window, cx| {
                            let subject_value = click_subject.read(cx).value().to_string();
                            let description_value = click_description.read(cx).value().to_string();
                            if !fields_filled(&subject_value, &description_value) {
                                return;
                            }
                            let url = build_github_issue_url(
                                &subject_value,
                                &description_value,
                                &version,
                                &build,
                                platform,
                            );
                            cx.open_url(&url);
                            window.close_dialog(cx);
                        }),
                ),
        )
        .into_any_element()
}

/// The GitHub new-issue URL: the user's subject and description, with this
/// build's version, build identifier, and platform appended so a report
/// always carries the exact build it came from.
fn build_github_issue_url(
    subject: &str,
    description: &str,
    version: &str,
    build: &str,
    platform: &str,
) -> String {
    let body = format!(
        "{description}\n\n\
        ---\n\
        **{APP_NAME} version:** {version}\n\
        **Build:** {build}\n\
        **Platform:** {platform}\n"
    );
    format!(
        "{ISSUE_URL_BASE}?title={}&body={}",
        encode(subject),
        encode(&body)
    )
}

fn encode(value: &str) -> String {
    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        ISSUE_URL_BASE, MenuSlot, REPORT_ISSUE_COMMAND_ID, ReportIssue, build_github_issue_url,
        fields_filled, register_commands, register_handler,
    };
    use crate::command::CommandRegistry;
    use gpui_kit::AppContext as _;
    use gpui_kit::component::WindowExt as _;

    #[test]
    fn the_url_targets_the_app_repos_new_issue_page() {
        let url = build_github_issue_url(
            "Pods panel crashes on refresh",
            "Steps: open a cluster, hit refresh twice.",
            "1.0.0",
            "2026-10-01, abc1234",
            "macos",
        );
        assert!(url.starts_with(ISSUE_URL_BASE));
    }

    #[test]
    fn the_url_is_percent_encoded_and_carries_the_subject_description_and_build_fields() {
        let url = build_github_issue_url(
            "Pods panel crashes on refresh",
            "Steps: open a cluster, hit refresh twice.",
            "1.0.0",
            "2026-10-01, abc1234",
            "macos",
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
            gpui_kit::init(cx);
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
        cx.update(|cx| register_handler(cx));
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
        cx.update(|cx| register_handler(cx));
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
}
