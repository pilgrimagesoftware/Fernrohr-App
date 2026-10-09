//! The way into `agent-mcp` from `main`: the app's endpoint startup. Kept
//! platform-neutral, so `main` needs no `cfg` of its own.

/// Starts the app's MCP endpoint; see `server::start`.
pub(crate) fn start(cx: &mut gpui_kit::App) {
    #[cfg(unix)]
    super::server::start(cx, super::tools::ToolRegistry::app());
    #[cfg(not(unix))]
    let _ = cx;
}
