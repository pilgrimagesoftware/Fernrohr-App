//! The cluster read tools (`agent-mcp`: Cluster session tools): contexts and
//! their status, discovered kinds, resources, and pod logs. None changes
//! anything, so none asks the user first.
//!
//! Each resolves its context through `cluster::session` and its kind through
//! `kinds::resolve`, so an unknown or disconnected context, or a kind the
//! context doesn't have, fails before any Kubernetes request.

mod contexts;
mod shape;

use super::tools::ToolRegistry;

/// Adds every read tool to `registry`.
pub(super) fn register(registry: &mut ToolRegistry) {
    contexts::register(registry);
}

#[cfg(test)]
mod test_support;
