//! What the UI calls each live agent (`mcp-connections` Security 5): the name
//! its MCP client reported, by [`AgentId`]. Self-reported - a label for the
//! user, never an identity: nothing decides anything by it.
//!
//! The MCP layer sets a label when a session opens and forgets it when the
//! session ends. A label never carries the session's secret identifier.

use super::AgentId;
use gpui_kit::{App, Global};
use std::collections::HashMap;

/// Live agents' self-reported names.
#[derive(Default)]
pub struct AgentLabels {
    names: HashMap<AgentId, Option<String>>,
}

impl Global for AgentLabels {}

impl AgentLabels {
    /// Records `agent` as live, with the name its client reported, if any.
    pub fn set(cx: &mut App, agent: AgentId, name: Option<String>) {
        cx.default_global::<Self>().names.insert(agent, name);
    }

    /// Forgets `agent`: its session ended.
    pub fn forget(cx: &mut App, agent: AgentId) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().names.remove(&agent);
        }
    }

    /// The name `agent`'s client reported: `None` when it reported none, or
    /// the agent isn't live.
    pub fn name(cx: &App, agent: AgentId) -> Option<String> {
        cx.try_global::<Self>()
            .and_then(|labels| labels.names.get(&agent).cloned().flatten())
    }
}
