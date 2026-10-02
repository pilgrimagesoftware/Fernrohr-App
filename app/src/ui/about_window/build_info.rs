//! Which binary is running: the version, the build it was made from, and the
//! one line a bug report needs.
//!
//! Separate from the window because it is the answer, not the presentation -
//! the same strings would serve a crash handler or a `--version` flag without
//! a window existing.

use crate::consts::APP_NAME;

/// Stamped by `build.rs` in place of the commit when there is no repository
/// to ask - a source tarball, or a machine without `git`. Must match the
/// `UNKNOWN` constant there.
const UNKNOWN_COMMIT: &str = "unknown";

/// The commit and build date `build.rs` stamped into this binary.
const BUILD_COMMIT: &str = env!("FERNROHR_BUILD_COMMIT");
const BUILD_DATE: &str = env!("FERNROHR_BUILD_DATE");

/// The running binary's released version - the crate's own version, so the
/// window cannot go stale against it.
pub(crate) fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The build identifier: the date the binary was built, and the commit it
/// was built from. Two builds of one version made from different commits
/// differ here, which is the whole point of showing it.
pub(crate) fn build_identifier() -> String {
    format_build(BUILD_DATE, BUILD_COMMIT)
}

/// Says the commit is unknown rather than leaving a blank where an
/// identifier belongs: a user reading "Build 2026-09-21" cannot tell whether
/// the commit was omitted or the window is broken.
fn format_build(date: &str, commit: &str) -> String {
    if commit == UNKNOWN_COMMIT {
        format!("{date}, commit unknown")
    } else {
        format!("{date}, {commit}")
    }
}

/// What the copy action puts on the clipboard - everything a bug report
/// needs about which binary was running, in one line.
pub(super) fn build_details() -> String {
    format!("{APP_NAME} {} ({})", version(), build_identifier())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_is_the_crates_own() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn a_known_commit_is_shown_with_the_date() {
        assert_eq!(
            format_build("2026-09-21", "1a2b3c4d5e6f"),
            "2026-09-21, 1a2b3c4d5e6f"
        );
    }

    #[test]
    fn an_unknown_commit_says_so_rather_than_leaving_a_blank() {
        let build = format_build("2026-09-21", UNKNOWN_COMMIT);
        assert!(
            build.starts_with("2026-09-21, "),
            "the date went missing: {build}"
        );
        assert!(
            build.contains("commit unknown"),
            "an unstamped commit did not say it was unknown: {build}"
        );
        assert!(
            !build.ends_with(&format!(", {UNKNOWN_COMMIT}")),
            "the raw stamp is shown where a commit belongs: {build}"
        );
    }

    #[test]
    fn the_build_identifier_names_this_build() {
        let build = build_identifier();
        assert!(
            build.starts_with(BUILD_DATE),
            "the build is not dated: {build}"
        );
        assert!(
            build.len() > BUILD_DATE.len(),
            "the build names no commit at all: {build}"
        );
    }

    #[test]
    fn the_copied_details_name_the_app_version_and_build() {
        let details = build_details();
        assert!(details.starts_with("Fernrohr"));
        assert!(
            details.contains(version()),
            "the version is missing: {details}"
        );
        assert!(
            details.contains(&build_identifier()),
            "the build is missing: {details}"
        );
    }
}
