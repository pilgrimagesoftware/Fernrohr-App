//! The "Report Issue" Help-menu command: opens the browser on a prefilled
//! GitHub issue, seeded with the same version and build identifier the About
//! window shows - a bug report always carries the exact build a user is
//! running, with no copy-pasting required.

use crate::command::{Command, CommandRegistry, MenuSlot};
use crate::consts::APP_NAME;
use crate::ui::about_window::{build_identifier, version};
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

/// Opens the browser on activation. App-wide, like the other Help items -
/// there is no window-specific state to act on.
pub fn register_handler(cx: &mut App) {
    cx.on_action(|_: &ReportIssue, cx: &mut App| {
        let url = build_github_issue_url(version(), &build_identifier(), std::env::consts::OS);
        cx.open_url(&url);
    });
}

/// The GitHub new-issue URL, prefilled with this build's version and build
/// identifier (date + commit, the same string the About window shows) so the
/// two can never drift apart.
fn build_github_issue_url(version: &str, build: &str, platform: &str) -> String {
    let title = "Bug report: [description]";
    let body = format!(
        "**{APP_NAME} version:** {version}\n\
**Build:** {build}\n\
**Platform:** {platform}\n\n\
**Steps to Reproduce**\n\n\n\
**Expected vs. Actual**\n"
    );
    format!(
        "{ISSUE_URL_BASE}?title={}&body={}",
        encode(title),
        encode(&body)
    )
}

fn encode(value: &str) -> String {
    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        ISSUE_URL_BASE, MenuSlot, REPORT_ISSUE_COMMAND_ID, build_github_issue_url,
        register_commands,
    };
    use crate::command::CommandRegistry;

    #[test]
    fn the_url_targets_the_app_repos_new_issue_page() {
        let url = build_github_issue_url("1.0.0", "2026-10-01, abc1234", "macos");
        assert!(url.starts_with(ISSUE_URL_BASE));
    }

    #[test]
    fn the_url_is_percent_encoded_and_carries_the_build_fields() {
        let url = build_github_issue_url("1.0.0", "2026-10-01, abc1234", "macos");
        assert!(url.contains("title=Bug%20report"));
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
}
