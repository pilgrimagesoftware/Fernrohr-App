//! Report Issue: a separate, non-modal window for reporting a bug or asking
//! for a feature (#241), with Subject focused as it opens (#249). Beside the
//! subject and description it offers the end of the application log (bugs
//! only, opt-in), screenshots picked from disk, and this build's diagnostics -
//! the same fields the About window shows, so the two can never drift apart.
//!
//! Report files the issue with the GitHub CLI when `gh` is installed and
//! signed in (#152, [`gh`]) - the window checks as it opens and says which way
//! it will go - and otherwise opens the browser on a prefilled GitHub issue,
//! as it does too when `gh` fails, saying why. Nothing is sent until Report.
//!
//! - [`window`]: the one window, opening it or raising it, and its state.
//! - [`render`]: what it draws.
//! - [`submit`]: Report - through `gh` or the browser - and what follows.
//! - [`body`]: the report type and the issue body and URL, pure.
//! - [`screenshots`]: which picked files are kept, and showing them.

use crate::command::{Command, CommandRegistry, MenuSlot};
use gpui_kit::*;

mod body;
mod gh;
mod render;
mod screenshots;
mod submit;
mod window;

actions!(
    report_issue,
    [ReportIssue, SendReport, CloseReport, AddScreenshots]
);

/// Opens Report Issue with its fields filled in - a panel error's Report…
/// (#177), so the error's text reaches the report without copying it there.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = report_issue, no_json)]
pub struct ReportError {
    pub subject: String,
    pub description: String,
}

impl ReportError {
    /// A report of the error `message`, with its technical `detail` - the
    /// subject its first line, the description both, the detail verbatim.
    pub fn of(message: &str, detail: Option<&str>) -> Self {
        let first_line = message.lines().next().unwrap_or_default().trim();
        let subject = match first_line.char_indices().nth(SUBJECT_LIMIT) {
            Some((cut, _)) => format!("{}\u{2026}", &first_line[..cut]),
            None => first_line.to_string(),
        };
        let description = match detail {
            Some(detail) => format!("{message}\n\n```\n{detail}\n```\n"),
            None => format!("{message}\n"),
        };
        Self {
            subject,
            description,
        }
    }
}

/// How many characters of an error's first line its report's subject keeps.
const SUBJECT_LIMIT: usize = 80;

pub const REPORT_ISSUE_COMMAND_ID: &str = "help.report_issue";
pub const REPORT_ISSUE_DEFAULT_BINDING: &str = "cmd-shift-/";

/// The Report Issue window's key context: where Report, Close and Add
/// Screenshots are bound.
pub(crate) const KEY_CONTEXT: &str = "ReportIssueWindow";

/// `help.report_issue` - a Help-menu item, palette entry and default
/// keybinding that all open the window - and the window's own commands.
pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: REPORT_ISSUE_COMMAND_ID,
        title: "Report Issue",
        default_binding: REPORT_ISSUE_DEFAULT_BINDING,
        context: None,
        action: Box::new(ReportIssue),
        menu: Some(MenuSlot::Help),
    });
    for (id, title, default_binding, action) in [
        (
            "report_issue.send",
            "Report Issue: Report",
            "secondary-enter",
            Box::new(SendReport) as Box<dyn Action>,
        ),
        (
            "report_issue.close",
            "Report Issue: Close",
            "escape",
            Box::new(CloseReport),
        ),
        (
            "report_issue.add_screenshots",
            "Report Issue: Add Screenshots…",
            "secondary-shift-a",
            Box::new(AddScreenshots),
        ),
    ] {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: Some(KEY_CONTEXT),
            action,
            menu: None,
        });
    }
}

/// Opens the window, or raises the open one, on activation. App-wide like the
/// other Help items: it belongs to no one window.
pub fn register_handler(cx: &mut App) {
    cx.on_action(|_: &ReportIssue, cx: &mut App| {
        // Deferred: action dispatch runs inside an update of the active
        // window, and raising a window from inside that update - the Report
        // Issue window itself, when the key is pressed there - would be a
        // reentrant update that fails silently.
        cx.defer(|cx| window::open_or_raise(None, cx));
    });
    cx.on_action(|report: &ReportError, cx: &mut App| {
        let report = report.clone();
        // Deferred, as above.
        cx.defer(move |cx| window::open_or_raise(Some(report), cx));
    });
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

/// Test-only: the open Report Issue window's Subject and Description, if one
/// is open - for tests elsewhere that open it from an error's Report….
#[cfg(test)]
pub(crate) fn test_fields(cx: &mut App) -> Option<(String, String)> {
    let (_, view) = window::test_view(cx)?;
    let view = view.read(cx);
    Some((
        view.subject.read(cx).value().to_string(),
        view.description.read(cx).value().to_string(),
    ))
}

#[cfg(test)]
mod tests;
