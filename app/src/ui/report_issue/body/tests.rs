use std::path::PathBuf;

use super::{
    Diagnostics, ISSUE_URL_BASE, IssueKind, LogSection, fields_filled, github_issue_url, issue_body,
};

pub(in crate::ui::report_issue) fn diagnostics() -> Diagnostics {
    Diagnostics {
        version: "1.0.0".into(),
        build: "2026-10-01, abc1234".into(),
        platform: "macos aarch64".into(),
    }
}

#[test]
fn a_plain_body_is_the_description_then_the_diagnostics() {
    let body = issue_body("Hit refresh twice.\n", &[], None, &diagnostics());
    assert_eq!(
        body,
        "Hit refresh twice.\n\n---\n**Fernrohr version:** 1.0.0\n\
         **Build:** 2026-10-01, abc1234\n**Platform:** macos aarch64\n"
    );
}

#[test]
fn screenshots_are_listed_by_name_before_the_diagnostics() {
    let shots = [
        PathBuf::from("/tmp/a/one.png"),
        PathBuf::from("/tmp/b/two.jpg"),
    ];
    let body = issue_body("Broken.", &shots, None, &diagnostics());
    let screenshots = body.find("### Screenshots").expect("a Screenshots section");
    assert!(
        body[screenshots..].contains("- `one.png`\n- `two.jpg`\n"),
        "{body}"
    );
    assert!(!body.contains("/tmp/a"), "only names, not paths: {body}");
    assert!(screenshots < body.find("---").unwrap());
}

#[test]
fn an_attached_log_is_collapsed_in_a_four_backtick_fence() {
    let log = LogSection::Tail("line with ``` backticks\nlast line\n".into());
    let body = issue_body("Broken.", &[], Some(&log), &diagnostics());
    assert!(
        body.contains(
            "### Logs\n\n<details>\n<summary>Application log</summary>\n\n\
         ````\nline with ``` backticks\nlast line\n````\n\n</details>\n"
        ),
        "{body}"
    );
    assert!(body.find("### Logs").unwrap() < body.find("---").unwrap());
}

#[test]
fn a_log_by_path_asks_the_reporter_to_attach_it() {
    let log = LogSection::Path(PathBuf::from("/logs/fernrohr.log"));
    let body = issue_body("Broken.", &[], Some(&log), &diagnostics());
    assert!(
        body.contains("Please attach `/logs/fernrohr.log` to this issue."),
        "{body}"
    );
    assert!(!body.contains("<details>"));
}

#[test]
fn screenshots_come_before_logs() {
    let log = LogSection::Tail("x".into());
    let body = issue_body("d", &[PathBuf::from("s.png")], Some(&log), &diagnostics());
    assert!(body.find("### Screenshots").unwrap() < body.find("### Logs").unwrap());
}

#[test]
fn the_url_targets_the_new_issue_page_encoded_and_labelled() {
    let url = github_issue_url(
        "Pods panel crashes",
        "Steps: refresh\ntwice.",
        IssueKind::Bug,
    );
    assert!(url.starts_with(ISSUE_URL_BASE), "{url}");
    assert!(url.contains("title=Pods%20panel%20crashes"), "{url}");
    assert!(url.contains("body=Steps%3A%20refresh%0Atwice%2E"), "{url}");
    assert!(url.ends_with("&labels=bug"), "{url}");
    assert!(!url.contains(' ') && !url.contains('\n'));

    let feature = github_issue_url("s", "b", IssueKind::Feature);
    assert!(feature.ends_with("&labels=enhancement"), "{feature}");
}

#[test]
fn only_a_bug_offers_the_log() {
    assert!(IssueKind::Bug.offers_log());
    assert!(!IssueKind::Feature.offers_log());
    assert_eq!(IssueKind::default(), IssueKind::Bug);
}

#[test]
fn report_is_disabled_until_both_fields_are_non_whitespace() {
    assert!(!fields_filled("", ""));
    assert!(!fields_filled("   ", "a description"));
    assert!(!fields_filled("a subject", "   "));
    assert!(fields_filled("a subject", "a description"));
}
