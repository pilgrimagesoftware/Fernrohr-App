//! The "Report Issue" Help-menu command: a dialog for a subject and
//! description, shown alongside the diagnostics a bug report needs (this
//! build's version, build identifier, and platform - the same fields the
//! About window shows, so the two can never drift apart).
//!
//! Report files the issue with the GitHub CLI when `gh` is installed and
//! signed in (#152, [`gh`]) - the dialog checks as it opens and says which way
//! it will go - and otherwise opens the browser on a prefilled GitHub issue,
//! as it does too when `gh` fails, saying why. Nothing is sent until Report.
//!
//! Modeled on `app/src/util/shell/tunnel_dialog.rs`'s `WindowExt`
//! `open_dialog`/`close_dialog` wiring.

use crate::command::{Command, CommandRegistry, MenuSlot};
use crate::consts::APP_NAME;
use crate::ui::about_window::{build_identifier, version};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::notification::Notification;
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

/// How Report will file the issue.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Route {
    /// Still finding out whether `gh` is ready; Report opens the browser
    /// until it knows.
    #[default]
    Checking,
    Gh,
    Browser,
}

/// The dialog's own state beside its fields: the route, and whether a report
/// is on its way through `gh`.
#[derive(Default)]
struct ReportState {
    route: Route,
    sending: bool,
}

/// The `PATH` tests find a fake `gh` on. Without it, a test build never looks
/// for `gh` - nor runs the login shell to find one - and Report opens the
/// browser.
#[cfg(test)]
pub(super) struct GhPathOverride(pub String);

#[cfg(test)]
impl Global for GhPathOverride {}

/// Where to look for `gh`: the login shell's `PATH` (resolved off the main
/// thread, inside the task), or a test's fake one. `None` means don't look.
fn gh_path(cx: &App) -> Option<Option<String>> {
    #[cfg(test)]
    {
        cx.try_global::<GhPathOverride>()
            .map(|path| Some(path.0.clone()))
    }
    #[cfg(not(test))]
    {
        let _ = cx;
        Some(None)
    }
}

fn resolve_path(path: Option<String>) -> String {
    path.unwrap_or_else(|| crate::util::login_env::login_path().to_string())
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
        let state = cx.new(|_| ReportState::default());
        check_route(state.clone(), window.window_handle(), cx);
        let version = version().to_string();
        let build = build_identifier();
        let platform = std::env::consts::OS;

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let form = Form {
                subject: subject.clone(),
                description: description.clone(),
                state: state.clone(),
                version: version.clone(),
                build: build.clone(),
                platform,
            };
            dialog
                .title("Report Issue")
                .w(px(440.))
                .content(move |content, _window, cx| content.child(form.clone().render(cx)))
        });
    });
}

/// Finds out, off the main thread, whether `gh` is ready, and redraws the
/// dialog to say which way Report will go.
fn check_route(state: Entity<ReportState>, window: AnyWindowHandle, cx: &mut App) {
    let Some(path) = gh_path(cx) else {
        state.update(cx, |state, _| state.route = Route::Browser);
        return;
    };
    let ready = cx.background_spawn(async move { gh::ready(&resolve_path(path)) });
    cx.spawn(async move |cx| {
        let route = if ready.await {
            Route::Gh
        } else {
            Route::Browser
        };
        state.update(cx, |state, cx| {
            state.route = route;
            cx.notify();
        });
        let _ = window.update(cx, |_, window, _| window.refresh());
    })
    .detach();
}

/// Section rule from Knot's `add-bug-reporting` proposal: Report is reachable
/// only once both fields carry more than whitespace.
fn fields_filled(subject: &str, description: &str) -> bool {
    !subject.trim().is_empty() && !description.trim().is_empty()
}

/// The dialog's fields, state and diagnostics.
#[derive(Clone)]
struct Form {
    subject: Entity<InputState>,
    description: Entity<TextareaState>,
    state: Entity<ReportState>,
    version: String,
    build: String,
    platform: &'static str,
}

impl Form {
    fn render(self, cx: &App) -> AnyElement {
        let theme = cx.theme().clone();
        let state = self.state.read(cx);
        let (route, sending) = (state.route, state.sending);
        let enabled = !sending
            && fields_filled(
                &self.subject.read(cx).value(),
                &self.description.read(cx).value(),
            );

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
        let route_line = match route {
            Route::Checking => "Checking for the GitHub CLI\u{2026}",
            Route::Gh => "Report files the issue with the GitHub CLI (gh).",
            Route::Browser => "Report opens a prefilled issue in your browser.",
        };

        div()
            .flex()
            .flex_col()
            .gap(crate::ui::space::spacing(cx).control_gap)
            .child(label("Subject"))
            .child(Input::new(&self.subject).w_full())
            .child(label("Description"))
            .child(Textarea::new(&self.description).w_full().h(px(120.)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded(px(6.))
                    .bg(theme.muted)
                    .child(diagnostic_line(format!("Version {}", self.version)))
                    .child(diagnostic_line(format!("Build {}", self.build)))
                    .child(diagnostic_line(format!("Platform {}", self.platform))),
            )
            .child(
                div()
                    .id(ROUTE_LINE)
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(route_line),
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
                            .label(if sending {
                                "Reporting\u{2026}"
                            } else {
                                "Report"
                            })
                            .primary()
                            .disabled(!enabled)
                            .on_click(move |_, window, cx| self.report(window, cx)),
                    ),
            )
            .into_any_element()
    }

    /// Report: through `gh` when it's ready, else - or when it fails - in the
    /// browser.
    fn report(&self, window: &mut Window, cx: &mut App) {
        let subject = self.subject.read(cx).value().to_string();
        let description = self.description.read(cx).value().to_string();
        if !fields_filled(&subject, &description) || self.state.read(cx).sending {
            return;
        }
        let body = issue_body(&description, &self.version, &self.build, self.platform);
        let url = build_github_issue_url(&subject, &body);
        let path = gh_path(cx);
        let (Route::Gh, Some(path)) = (self.state.read(cx).route, path) else {
            cx.open_url(&url);
            window.close_dialog(cx);
            return;
        };
        self.state.update(cx, |state, cx| {
            state.sending = true;
            cx.notify();
        });
        window.refresh();
        let filed =
            cx.background_spawn(async move { gh::create(&resolve_path(path), &subject, &body) });
        let handle = window.window_handle();
        cx.spawn(async move |cx| {
            let filed = filed.await;
            let _ = handle.update(cx, |_, window, cx| {
                window.close_dialog(cx);
                let notification = match filed {
                    Ok(issue) => {
                        Notification::success(format!("Filed {issue}")).title("Issue reported")
                    }
                    Err(reason) => {
                        cx.open_url(&url);
                        Notification::warning(format!(
                            "The GitHub CLI couldn't file it ({reason}), so it opened in \
                             your browser instead."
                        ))
                        .title("Report Issue")
                    }
                };
                window.push_notification(notification, cx);
            });
        })
        .detach();
    }
}

/// The dialog's line saying how Report will file the issue, for tests.
const ROUTE_LINE: &str = "report-issue-route";

/// The issue body: the user's description, with this build's version, build
/// identifier, and platform appended so a report always carries the exact
/// build it came from.
fn issue_body(description: &str, version: &str, build: &str, platform: &str) -> String {
    format!(
        "{description}\n\n\
        ---\n\
        **{APP_NAME} version:** {version}\n\
        **Build:** {build}\n\
        **Platform:** {platform}\n"
    )
}

/// The GitHub new-issue URL for `subject` and `body`.
fn build_github_issue_url(subject: &str, body: &str) -> String {
    format!(
        "{ISSUE_URL_BASE}?title={}&body={}",
        encode(subject),
        encode(body)
    )
}

fn encode(value: &str) -> String {
    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

mod gh;

#[cfg(test)]
mod tests;
