//! The two ways into `agent-mcp` from `main`: the `fernrohr mcp` subcommand,
//! and the app's endpoint startup. Kept platform-neutral, so `main` needs no
//! `cfg` of its own.

/// The subcommand an MCP client runs: `fernrohr mcp`.
const SUBCOMMAND: &str = "mcp";

/// Runs `fernrohr mcp` and returns its exit code when `args` (the process's,
/// program name first) ask for it; `None` means the app should start as usual.
pub(crate) fn run_subcommand(mut args: impl Iterator<Item = std::ffi::OsString>) -> Option<i32> {
    if args.nth(1)? != SUBCOMMAND {
        return None;
    }
    #[cfg(unix)]
    return Some(super::adapter::run());
    #[cfg(not(unix))]
    {
        eprintln!("fernrohr mcp: the MCP endpoint needs Unix-domain sockets");
        Some(1)
    }
}

/// Starts the app's MCP endpoint; see `server::start`.
pub(crate) fn start(cx: &mut gpui_kit::App) {
    #[cfg(unix)]
    super::server::start(cx, super::tools::ToolRegistry::app());
    #[cfg(not(unix))]
    let _ = cx;
}

#[cfg(test)]
mod tests {
    use super::run_subcommand;
    use std::ffi::OsString;

    fn args(list: &[&str]) -> impl Iterator<Item = OsString> {
        list.iter()
            .map(OsString::from)
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn only_the_mcp_subcommand_skips_the_app() {
        assert_eq!(run_subcommand(args(&["fernrohr"])), None);
        assert_eq!(run_subcommand(args(&["fernrohr", "--other"])), None);
        assert_eq!(run_subcommand(args(&["fernrohr", "mcpx"])), None);
    }
}
