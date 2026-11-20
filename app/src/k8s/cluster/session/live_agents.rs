//! Which agents are live (`mcp-connections` design decision 3, review of
//! App#434 F1/F2): every MCP session, by its in-app [`AgentId`], from the moment
//! the MCP layer hands out its identifier until its end is handled - both on the
//! main thread. [`ClusterRegistry::hold`] refuses an agent that isn't live, so a
//! call that outlives its session (an approval answered after the client left)
//! can't leave a hold no session will ever release.
//!
//! Each live agent also carries the name its MCP client reported: a label for
//! the user, never an identity, and never used to decide anything.

use super::{AgentId, ClusterRegistry};
use gpui_kit::{App, Global};
use std::collections::HashMap;
use std::time::Duration;

/// The live agents and their self-reported names.
#[derive(Default)]
pub struct LiveAgents {
    agents: HashMap<AgentId, Option<String>>,
}

impl Global for LiveAgents {}

impl LiveAgents {
    /// Records `agent` as live, reporting itself as `name`.
    pub fn open(cx: &mut App, agent: AgentId, name: Option<String>) {
        cx.default_global::<Self>().agents.insert(agent, name);
    }

    /// Ends `agent`, if it is live: it can't hold anything from now on, and its
    /// holds linger for `grace`, then go (`ClusterRegistry::linger_agent`).
    pub fn end(cx: &mut App, agent: AgentId, grace: Duration) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().agents.remove(&agent);
        }
        ClusterRegistry::linger_agent(cx, agent, grace);
    }

    /// Ends every live agent, as stopping the MCP server does. Covers agents
    /// whose own end hadn't reached the main thread yet.
    pub fn end_all(cx: &mut App, grace: Duration) {
        let agents: Vec<AgentId> = cx
            .try_global::<Self>()
            .map(|live| live.agents.keys().copied().collect())
            .unwrap_or_default();
        for agent in agents {
            Self::end(cx, agent, grace);
        }
    }

    /// Whether `agent`'s session is live.
    pub fn is_live(cx: &App, agent: AgentId) -> bool {
        cx.try_global::<Self>()
            .is_some_and(|live| live.agents.contains_key(&agent))
    }

    /// The name `agent`'s client reported: `None` when it reported none, or the
    /// agent isn't live.
    // UNWIRED(#367): read by the Connections window and status bar (sections 4
    // and 5).
    #[allow(dead_code)]
    pub fn name(cx: &App, agent: AgentId) -> Option<String> {
        cx.try_global::<Self>()
            .and_then(|live| live.agents.get(&agent).cloned().flatten())
    }
}
