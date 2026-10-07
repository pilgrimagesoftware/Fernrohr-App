//! Filing a report with the GitHub CLI (#152): whether `gh` is on the user's
//! login-shell `PATH` and signed in to github.com, and `gh issue create` for a
//! report. Each call blocks on a child process, so the dialog runs them off
//! the main thread; `path` is the `PATH` to find `gh` on - the login shell's
//! in the app (`util::login_env`), a fake's in tests.

use std::io::Write as _;
use std::process::{Command, Stdio};

/// The repository reports are filed against.
pub(super) const REPO: &str = "pilgrimagesoftware/Fernrohr-App";

/// Whether `gh` is on `path` and signed in to github.com - what decides that
/// Report files the issue itself rather than opening the browser.
pub(super) fn ready(path: &str) -> bool {
    Command::new("gh")
        .env("PATH", path)
        .args(["auth", "status", "--hostname", "github.com"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Files an issue titled `title` with `body` against [`REPO`], returning the
/// new issue's URL - or why `gh` couldn't, in its own words. The body goes
/// over stdin, so no length or quoting limit applies.
pub(super) fn create(path: &str, title: &str, body: &str) -> Result<String, String> {
    let mut child = Command::new("gh")
        .env("PATH", path)
        .args([
            "issue",
            "create",
            "--repo",
            REPO,
            "--title",
            title,
            "--body-file",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("couldn't run gh: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(body.as_bytes())
            .map_err(|error| format!("couldn't send the report to gh: {error}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("gh didn't finish: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason = stderr.trim();
        return Err(if reason.is_empty() {
            format!("gh exited with {}", output.status)
        } else {
            reason.to_string()
        });
    }
    // `gh issue create` ends with the new issue's URL.
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .rfind(|line| line.starts_with("https://"))
        .map(str::to_string)
        .ok_or_else(|| "gh didn't say where it filed the issue".to_string())
}

#[cfg(test)]
pub(super) mod tests;
