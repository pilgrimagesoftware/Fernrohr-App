use super::{Invocation, parse};
use std::ffi::OsString;

fn run(args: &[&str]) -> Invocation {
    parse(args.iter().map(OsString::from))
}

/// The printed text, stream and exit code, for an invocation that prints.
fn printed(args: &[&str]) -> (String, bool, i32) {
    match run(args) {
        Invocation::Print {
            text,
            to_stderr,
            code,
        } => (text, to_stderr, code),
        other => panic!("{args:?} should print, got {other:?}"),
    }
}

#[test]
fn no_arguments_start_the_app() {
    assert_eq!(run(&[]), Invocation::StartApp);
}

#[test]
fn every_help_spelling_prints_usage_to_stdout() {
    for flag in ["--help", "-h", "help"] {
        let (text, to_stderr, code) = printed(&[flag]);
        assert_eq!((to_stderr, code), (false, 0), "{flag}");
        assert!(text.contains("Usage:"), "{flag}: {text}");
        assert!(text.contains("fernrohr mcp"), "{flag}: {text}");
    }
}

#[test]
fn version_prints_the_crate_version_to_stdout() {
    for flag in ["--version", "-V"] {
        let (text, to_stderr, code) = printed(&[flag]);
        assert_eq!((to_stderr, code), (false, 0), "{flag}");
        assert_eq!(text, format!("fernrohr {}\n", env!("CARGO_PKG_VERSION")));
    }
}

#[test]
fn mcp_runs_the_adapter() {
    assert_eq!(run(&["mcp"]), Invocation::Mcp);
}

#[test]
fn mcp_help_prints_the_mcp_usage_without_running_the_adapter() {
    for flag in ["--help", "-h"] {
        let (text, to_stderr, code) = printed(&["mcp", flag]);
        assert_eq!((to_stderr, code), (false, 0), "{flag}");
        assert!(text.starts_with("Usage: fernrohr mcp"), "{flag}: {text}");
    }
}

/// An error goes to stderr with exit code 2, names the argument it rejects,
/// and is followed by the usage.
fn assert_rejects(args: &[&str], offending: &str) {
    let (text, to_stderr, code) = printed(args);
    assert_eq!((to_stderr, code), (true, 2), "{args:?}");
    let first_line = text.lines().next().unwrap_or_default();
    assert!(
        first_line.contains(&format!("'{offending}'")),
        "{args:?}: {first_line}"
    );
    assert!(text.contains("Usage:"), "{args:?}: {text}");
}

#[test]
fn an_unknown_flag_is_rejected() {
    assert_rejects(&["--verbose"], "--verbose");
}

#[test]
fn an_unknown_subcommand_is_rejected() {
    assert_rejects(&["serve"], "serve");
    assert_rejects(&["mcpx"], "mcpx");
}

#[test]
fn an_extra_argument_after_mcp_is_rejected() {
    assert_rejects(&["mcp", "serve"], "serve");
    assert_rejects(&["mcp", "--help", "extra"], "--help");
}

#[test]
fn an_extra_argument_after_help_or_version_is_rejected() {
    assert_rejects(&["--help", "extra"], "extra");
    assert_rejects(&["--version", "--help"], "--help");
}

#[cfg(target_os = "macos")]
#[test]
fn the_macos_process_serial_number_still_starts_the_app() {
    assert_eq!(run(&["-psn_0_123"]), Invocation::StartApp);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn a_process_serial_number_is_unrecognized_off_macos() {
    assert_rejects(&["-psn_0_123"], "-psn_0_123");
}

#[cfg(unix)]
#[test]
fn a_non_utf8_argument_is_rejected_not_a_panic() {
    use std::os::unix::ffi::OsStringExt;
    let arg = OsString::from_vec(vec![b'-', 0xff]);
    match parse([arg]) {
        Invocation::Print {
            to_stderr, code, ..
        } => assert_eq!((to_stderr, code), (true, 2)),
        other => panic!("expected an error, got {other:?}"),
    }
}
