//! `command-line` on the built binary (cli-help-and-shutdown task 1.2): help
//! and an unrecognized flag print and exit before the app does anything, so
//! they leave no log, data or preference directory behind.

mod support;

use support::FreshHome;

#[test]
fn help_prints_usage_and_creates_nothing() {
    let home = FreshHome::new();
    let output = home.command(&["--help"]).output().expect("run fernrohr");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage:"), "{stdout}");
    assert!(output.stderr.is_empty());
    assert_eq!(home.created(), Vec::<std::path::PathBuf>::new());
}

#[test]
fn an_unknown_flag_is_an_error_and_creates_nothing() {
    let home = FreshHome::new();
    let output = home.command(&["--bogus"]).output().expect("run fernrohr");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("fernrohr: unrecognized argument '--bogus'"),
        "{stderr}"
    );
    assert!(stderr.contains("Usage:"), "{stderr}");
    assert_eq!(home.created(), Vec::<std::path::PathBuf>::new());
}

#[test]
fn mcp_help_prints_the_mcp_usage_and_creates_nothing() {
    let home = FreshHome::new();
    let output = home
        .command(&["mcp", "--help"])
        .output()
        .expect("run fernrohr");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("Usage: fernrohr mcp"), "{stdout}");
    assert_eq!(home.created(), Vec::<std::path::PathBuf>::new());
}
