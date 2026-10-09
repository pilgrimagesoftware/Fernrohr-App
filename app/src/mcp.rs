//! `agent-mcp` (Fernrohr#189): an MCP server that lets a local agent use the
//! app's own cluster sessions and panels.
//!
//! Two halves of one binary, joined by a user-owned Unix-domain socket:
//!
//! - The app ([`server`]) binds the socket in its runtime directory, writes a
//!   per-launch token beside it ([`endpoint`]), and answers a small internal
//!   RPC ([`protocol`], framed by [`frame`]) from the tools it registered
//!   ([`tools`]; the panel and saved-layout tools are [`navigate`]'s).
//! - `fernrohr mcp` ([`adapter`]) is the command an MCP client launches. It
//!   speaks MCP on stdio through `rmcp`, and forwards each request to the
//!   running app over that socket ([`client`]), one connection per request.
//!
//! Every failure a client can see is a [`error::ToolError`], built so no
//! credential, kubeconfig content or upstream detail reaches it ([`redact`]).
//!
//! Unix only: the endpoint is a Unix-domain socket. Elsewhere the app serves
//! nothing and `fernrohr mcp` reports itself unsupported.

#[cfg(unix)]
mod actions;
#[cfg(unix)]
mod adapter;
#[cfg(unix)]
pub(crate) mod approval;
#[cfg(unix)]
mod client;
#[cfg(unix)]
mod cluster;
#[cfg(unix)]
mod endpoint;
mod entry;
#[cfg(unix)]
mod error;
#[cfg(unix)]
mod foreground;
#[cfg(unix)]
mod frame;
#[cfg(all(test, unix))]
mod integration;
#[cfg(unix)]
mod kinds;
#[cfg(unix)]
mod names;
#[cfg(unix)]
mod navigate;
#[cfg(unix)]
mod protocol;
#[cfg(unix)]
mod read;
#[cfg(unix)]
mod redact;
#[cfg(unix)]
mod server;
pub(crate) mod setup;
#[cfg(all(test, unix))]
mod test_support;
#[cfg(unix)]
mod tools;

pub(crate) use entry::{run_subcommand, start};
