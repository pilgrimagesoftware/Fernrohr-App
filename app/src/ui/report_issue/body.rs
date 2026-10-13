//! What a report says: its type, and the issue body and browser URL built from
//! the window's fields. Pure, so every combination is unit-tested without a
//! window.

use std::path::{Path, PathBuf};

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

use crate::consts::APP_NAME;

const ISSUE_URL_BASE: &str = "https://github.com/pilgrimagesoftware/Fernrohr-App/issues/new";

/// Whether a report is a bug or a feature request: its label, its fields'
/// placeholders, and whether it offers the log.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum IssueKind {
    #[default]
    Bug,
    Feature,
}

impl IssueKind {
    /// In the radio group's order.
    pub(super) const ALL: [IssueKind; 2] = [IssueKind::Bug, IssueKind::Feature];

    pub(super) fn label(self) -> &'static str {
        match self {
            IssueKind::Bug => "Bug",
            IssueKind::Feature => "Feature request",
        }
    }

    /// The GitHub label the issue is filed with.
    pub(super) fn github_label(self) -> &'static str {
        match self {
            IssueKind::Bug => "bug",
            IssueKind::Feature => "enhancement",
        }
    }

    pub(super) fn subject_placeholder(self) -> &'static str {
        match self {
            IssueKind::Bug => "What went wrong",
            IssueKind::Feature => "What you'd like Fernrohr to do",
        }
    }

    pub(super) fn description_placeholder(self) -> &'static str {
        match self {
            IssueKind::Bug => "Steps to reproduce, and what you expected instead.",
            IssueKind::Feature => "What you're trying to do, and how this would help.",
        }
    }

    /// Only a bug offers the application log.
    pub(super) fn offers_log(self) -> bool {
        match self {
            IssueKind::Bug => true,
            IssueKind::Feature => false,
        }
    }
}

/// The build a report came from, as the About window shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Diagnostics {
    pub(super) version: String,
    pub(super) build: String,
    /// The operating system and architecture, e.g. `macos aarch64`.
    pub(super) platform: String,
}

impl Diagnostics {
    pub(super) fn current() -> Self {
        Self {
            version: crate::ui::about_window::version().to_string(),
            build: crate::ui::about_window::build_identifier(),
            platform: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        }
    }
}

/// What the Logs section carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum LogSection {
    /// The end of the log, read when the report was sent.
    Tail(String),
    /// The log's path, for the reporter to attach by hand: the browser page
    /// can't carry the text, or the file couldn't be read.
    Path(PathBuf),
}

/// The issue body: the description, the Screenshots and Logs sections when
/// there are any, then this build's diagnostics.
pub(super) fn issue_body(
    description: &str,
    screenshots: &[PathBuf],
    log: Option<&LogSection>,
    diagnostics: &Diagnostics,
) -> String {
    let mut body = description.trim_end().to_string();
    if !screenshots.is_empty() {
        body.push_str(
            "\n\n### Screenshots\n\nThe reporter is adding these screenshots to the issue:\n\n",
        );
        for path in screenshots {
            body.push_str(&format!("- `{}`\n", file_name(path)));
        }
    }
    match log {
        Some(LogSection::Tail(tail)) => body.push_str(&format!(
            "\n\n### Logs\n\n<details>\n<summary>Application log</summary>\n\n````\n{}\n````\n\n</details>\n",
            tail.trim_end()
        )),
        Some(LogSection::Path(path)) => body.push_str(&format!(
            "\n\n### Logs\n\nPlease attach `{}` to this issue.\n",
            path.display()
        )),
        None => {}
    }
    body.push_str(&format!(
        "\n\n---\n**{APP_NAME} version:** {}\n**Build:** {}\n**Platform:** {}\n",
        diagnostics.version, diagnostics.build, diagnostics.platform
    ));
    body
}

/// The GitHub new-issue page for `subject` and `body`, labelled for `kind`.
pub(super) fn github_issue_url(subject: &str, body: &str, kind: IssueKind) -> String {
    format!(
        "{ISSUE_URL_BASE}?title={}&body={}&labels={}",
        encode(subject),
        encode(body),
        encode(kind.github_label())
    )
}

/// A path's file name, or the whole path when it has none.
pub(super) fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Report is reachable only once both fields carry more than whitespace.
pub(super) fn fields_filled(subject: &str, description: &str) -> bool {
    !subject.trim().is_empty() && !description.trim().is_empty()
}

fn encode(value: &str) -> String {
    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

#[cfg(test)]
pub(super) mod tests;
