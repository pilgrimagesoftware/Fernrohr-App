//! Who keeps a context's connection open (`mcp-connections` design decision 2):
//! a window, or an agent's MCP session. A session lives while any holder of
//! either kind remains.
//!
//! An agent is named here by an [`AgentId`], an in-app handle, never by its MCP
//! session identifier: that identifier is a secret (`mcp-connections` Security 4)
//! and stays in the MCP layer, which maps it to an `AgentId`.

use gpui_kit::WindowId;
use std::sync::atomic::{AtomicU64, Ordering};

/// An agent's in-app handle: unique for the app's lifetime and never reused, so a
/// hold can't pass from an ended agent to a new one. Carries no secret.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AgentId(u64);

impl AgentId {
    /// A fresh handle, distinct from every one handed out before.
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// An ended agent session's hold, kept only until its grace period ends
/// (`mcp-connections` design decision 3). Unique and never reused, so no later
/// session can claim it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LingerId(u64);

impl LingerId {
    fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// One holder of a context's connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Holder {
    /// A window that uses the context: its capsule, Resource panel and panels.
    Window(WindowId),
    /// An agent's MCP session, approved for the context or sharing a window's.
    Agent(AgentId),
    /// What an ended agent session's hold becomes: it keeps the connection open
    /// until its grace period ends, and no session can claim it.
    Lingering(LingerId),
}

impl From<WindowId> for Holder {
    fn from(window: WindowId) -> Self {
        Self::Window(window)
    }
}

impl From<AgentId> for Holder {
    fn from(agent: AgentId) -> Self {
        Self::Agent(agent)
    }
}

impl Holder {
    /// A fresh lingering hold.
    pub(super) fn lingering() -> Self {
        Self::Lingering(LingerId::next())
    }

    /// Whether this holder is a window.
    pub fn is_window(&self) -> bool {
        matches!(self, Self::Window(_))
    }
}
