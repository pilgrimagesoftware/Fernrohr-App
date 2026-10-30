//! The two ways into `agent-mcp` from `main`: the `fernrohr mcp` command that
//! `cli::parse` dispatches to, and the app's endpoint startup. Kept
//! platform-neutral, so `main` needs no `cfg` of its own.

/// Runs `fernrohr mcp`, which `cli::parse` has already recognized, and
/// returns its exit code.
pub(crate) fn run_command() -> i32 {
    #[cfg(unix)]
    return super::adapter::run();
    #[cfg(not(unix))]
    {
        eprintln!("fernrohr mcp: the MCP endpoint needs Unix-domain sockets");
        1
    }
}

/// Starts the app's MCP endpoint, unless it already runs; see
/// `server::start`.
pub(crate) fn start(cx: &mut gpui_kit::App) {
    #[cfg(unix)]
    super::server::start(cx, super::tools::ToolRegistry::app());
    #[cfg(not(unix))]
    let _ = cx;
}

/// Stops the app's MCP endpoint, if it runs; see `server::stop`. Quiet mode
/// (#236) does this when the last window closes with the background setting
/// off.
pub(crate) fn stop(cx: &mut gpui_kit::App) {
    #[cfg(unix)]
    super::server::stop(cx);
    #[cfg(not(unix))]
    let _ = cx;
}
