//! The Connections window (`mcp-connections` section 4, Fernrohr#367): every
//! cluster connection open in Fernrohr, whichever window or MCP session holds it,
//! with its state, tunnel and holders, and the actions to open it in a window,
//! disconnect it, or end an agent's hold - without a window per context.
//!
//! - [`rows`] reads the registry into plain rows (no rendering, no I/O).
//! - [`window`] owns the singleton window, its live updates and its selection.
//! - [`actions`] is what Open in Window, Disconnect and End Agent's Hold do.
//! - [`render`] draws the list, the row icon buttons and the hint row.
//! - [`commands`] registers the commands.

mod actions;
mod agent_picker;
mod commands;
mod render;
mod rows;
mod window;

pub use commands::{ConnectionsOpen, register_commands};
pub(crate) use window::{open_or_focus, select_context};

/// Test-only: `None` while the window is closed, else its selected context.
#[cfg(test)]
pub(crate) fn test_selection(cx: &mut gpui_kit::App) -> Option<Option<String>> {
    window::test_view(cx).map(|view| view.read(cx).selected.clone())
}

#[cfg(test)]
mod tests;
