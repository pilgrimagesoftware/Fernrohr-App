//! Which agents share a context the window uses (`app-shell`: Status bar
//! capsule marks agent-held contexts): their self-reported names, for the
//! capsule's agent icon and its Disconnect confirmation. A name is a label the
//! agent's MCP client gave itself, never an identity, so it is always shown as
//! self-reported and never decides anything.

use crate::k8s::cluster::session::{ClusterRegistry, Holder, LiveAgents};
use gpui_kit::App;

/// The self-reported names of the agents holding `context_name`, in holder
/// order: `None` for an agent that reported none. Empty when no agent does.
pub(crate) fn agent_names(cx: &App, context_name: &str) -> Vec<Option<String>> {
    ClusterRegistry::holders(cx, context_name)
        .into_iter()
        .filter_map(|holder| match holder {
            Holder::Agent(agent) => Some(LiveAgents::name(cx, agent)),
            Holder::Window(_) | Holder::Lingering(_) => None,
        })
        .collect()
}

/// One agent as a sentence names it.
pub(crate) fn describe(name: &Option<String>) -> String {
    match name {
        Some(name) => format!("an agent reporting itself as \u{201c}{name}\u{201d}"),
        None => "an unnamed agent".to_string(),
    }
}

/// The agent icon's tooltip: who else uses the context. `None` with no agents.
pub(crate) fn tooltip(names: &[Option<String>]) -> Option<String> {
    (!names.is_empty()).then(|| {
        let agents: Vec<String> = names.iter().map(describe).collect();
        format!("Also used by {}", agents.join(", "))
    })
}

/// The capsule's agent icon's debug selector.
pub(crate) fn agent_icon_selector(context_name: &str) -> String {
    format!("status-agents-{context_name}")
}

#[cfg(test)]
mod tests;
