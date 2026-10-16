//! `command-line` (cli-help-and-shutdown D1): what the `fernrohr` executable
//! does with its arguments. A pure parse, so every case is a unit test rather
//! than a spawned process; `main` acts on the [`Invocation`].
//!
//! The app starts only with no arguments. Help, version and every argument it
//! doesn't recognize print and exit before `main` touches tunnels, the app log
//! or a window (D2), so an agent probing `fernrohr --help` never gets a second
//! Fernrohr in front of the user.

use std::ffi::{OsStr, OsString};

/// The subcommand an MCP client runs: `fernrohr mcp`.
const MCP: &str = "mcp";

/// What `main` should do for the process's arguments.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Invocation {
    /// No arguments: start the app.
    StartApp,
    /// `fernrohr mcp`: run the stdio MCP adapter.
    Mcp,
    /// Print `text` to stdout, or stderr when `to_stderr`, and exit with `code`.
    Print {
        text: String,
        to_stderr: bool,
        code: i32,
    },
}

/// The exit code for an invocation the executable doesn't recognize.
const USAGE_ERROR: i32 = 2;

/// Parses the process's arguments, program name excluded.
pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Invocation {
    let args: Vec<OsString> = args
        .into_iter()
        .filter(|arg| !is_launcher_argument(arg))
        .collect();
    let Some((command, rest)) = args.split_first() else {
        return Invocation::StartApp;
    };
    match command.to_str() {
        Some("help" | "-h" | "--help") => only(rest, || stdout(usage())),
        Some("-V" | "--version") => only(rest, || stdout(version())),
        Some(MCP) => match rest {
            [] => Invocation::Mcp,
            [flag] if is_help(flag) => stdout(MCP_USAGE.to_string()),
            [extra, ..] => error(&format!(
                "unexpected argument '{}' after '{MCP}'",
                extra.to_string_lossy()
            )),
        },
        _ => error(&format!(
            "unrecognized argument '{}'",
            command.to_string_lossy()
        )),
    }
}

/// `printed`, unless the command was given more arguments than it takes.
fn only(rest: &[OsString], printed: impl FnOnce() -> Invocation) -> Invocation {
    match rest.first() {
        None => printed(),
        Some(extra) => error(&format!(
            "unexpected argument '{}'",
            extra.to_string_lossy()
        )),
    }
}

fn is_help(arg: &OsStr) -> bool {
    arg == "-h" || arg == "--help"
}

/// An argument the platform's own launcher adds, not the user: the process
/// serial number older macOS passes to apps launched from the Finder.
fn is_launcher_argument(arg: &OsStr) -> bool {
    cfg!(target_os = "macos") && arg.to_string_lossy().starts_with("-psn_")
}

fn stdout(text: String) -> Invocation {
    Invocation::Print {
        text,
        to_stderr: false,
        code: 0,
    }
}

fn error(message: &str) -> Invocation {
    Invocation::Print {
        text: format!("fernrohr: {message}\n\n{}", usage()),
        to_stderr: true,
        code: USAGE_ERROR,
    }
}

fn version() -> String {
    format!("fernrohr {}\n", env!("CARGO_PKG_VERSION"))
}

fn usage() -> String {
    format!(
        "Fernrohr {}: a desktop client for Kubernetes.

Usage:
  fernrohr              Start the app.
  fernrohr mcp          Run the stdio MCP server an agent's MCP client launches.
                        It needs a running Fernrohr. See `fernrohr mcp --help`.
  fernrohr --help       Print this help (also -h, help).
  fernrohr --version    Print the version (also -V).
",
        env!("CARGO_PKG_VERSION")
    )
}

/// `fernrohr mcp --help` (D4).
const MCP_USAGE: &str = "Usage: fernrohr mcp

Runs Fernrohr's MCP server on standard input and output, for an agent's MCP
client to launch. It talks to the running Fernrohr app over a local socket, and
never starts the app or opens a window: with no app running, each tool call
reports the app as unavailable.

It exits when the client closes its input, signals it, or exits.
";

#[cfg(test)]
mod tests;
