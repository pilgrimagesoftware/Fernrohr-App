//! `connect_context` (`agent-mcp`: Connect context tool, Connecting requires
//! approval, Connect reports the settled state; `mcp-connect-and-focus`
//! D1-D3): connects a kubeconfig context in the frontmost Fernrohr window, so
//! an agent can bring a cluster online without the user leaving it.
//!
//! In order: the name is checked against the kubeconfig; a context the
//! frontmost window already holds is just brought forward, with no question;
//! anything else is asked through the approval gate - naming the context and
//! its tunnel - before `util::shell::connect_context` adds it as the status
//! bar's add-context control does. Then the tool watches the connection for a
//! bounded time and reports the first settled state, or `connecting` when the
//! wait ends; the connection carries on either way.

use super::super::approval::{ApprovalRequest, approve};
use super::super::cluster::{ContextStatus, kubeconfig_contexts, status};
use super::super::error::ToolError;
use super::super::redact::redact;
use super::super::tools::{ToolContext, ToolKind, ToolOutput, ToolRegistry, ToolResult};
use crate::config::tunnels::TunnelKind;
use crate::consts::MCP_CONNECT_POLL;
use crate::k8s::cluster::context_health::ContextHealth;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::tunnel::store::TunnelStore;
use crate::util::shell;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use tokio::time::Instant;

pub(super) const CONNECT_CONTEXT: &str = "connect_context";

pub(super) fn register(registry: &mut ToolRegistry) {
    registry.add(
        CONNECT_CONTEXT,
        "Connect context",
        "Connects a kubeconfig context in the frontmost Fernrohr window, as its add-context \
         control does, after the user approves it in the app, and brings that window forward. \
         A context the window already has is just brought forward. Waits briefly, then \
         reports `connected`, `waiting_for_tunnel`, `awaiting_confirmation` (the user must \
         confirm a manual tunnel), `failed` with a reason, or `connecting` if it is still \
         going; `list_contexts` reports later changes.",
        ToolKind::Navigate,
        connect_context,
    );
}

/// `connect_context`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ConnectContextInput {
    /// The kubeconfig context to connect, as `list_contexts` reports it.
    context: String,
}

async fn connect_context(input: ConnectContextInput, tools: ToolContext) -> ToolResult {
    let context = input.context;
    let known = kubeconfig_contexts(tools.kubeconfig.clone())
        .await
        .is_some_and(|names| names.contains(&context));
    if !known {
        return Err(ToolError::UnknownContext { context });
    }

    let name = context.clone();
    if tools
        .foreground
        .run(move |cx| shell::focus_if_held(&name, cx))
        .await?
    {
        let (state, reason) = state_of(&tools, &context).await?;
        return Ok(output(&context, state, reason, true));
    }

    let tunnel = bound_tunnel(tools.tunnels.clone(), &context).await;
    approve(
        &tools.approvals,
        ApprovalRequest::connect(
            &context,
            tunnel.as_ref().map(|(name, kind)| (name.as_str(), *kind)),
        ),
    )
    .await?;

    let name = context.clone();
    tools
        .foreground
        .run(move |cx| shell::connect_context(name, cx))
        .await?;
    let (state, reason) = settle(&tools, &context).await?;
    Ok(output(&context, state, reason, false))
}

/// Watches `context` until it settles or `tools.connect_settle` passes
/// (`mcp-connect-and-focus` D3). Awaiting a manual tunnel's confirmation is
/// settled at once: only the user can end it.
async fn settle(
    tools: &ToolContext,
    context: &str,
) -> Result<(ContextStatus, Option<String>), ToolError> {
    let deadline = Instant::now() + tools.connect_settle;
    loop {
        let (state, reason) = state_of(tools, context).await?;
        let settled = match state {
            ContextStatus::Connected
            | ContextStatus::WaitingForTunnel
            | ContextStatus::AwaitingConfirmation
            | ContextStatus::Paused
            | ContextStatus::Failed => true,
            ContextStatus::Connecting | ContextStatus::NotOpen => false,
        };
        if settled {
            return Ok((state, reason));
        }
        if Instant::now() >= deadline {
            return Ok((ContextStatus::Connecting, None));
        }
        tokio::time::sleep(MCP_CONNECT_POLL).await;
    }
}

/// `context`'s state now, and a failure's reason with anything that looks
/// like a credential taken out.
async fn state_of(
    tools: &ToolContext,
    context: &str,
) -> Result<(ContextStatus, Option<String>), ToolError> {
    let context = context.to_string();
    tools
        .foreground
        .run(move |cx| {
            let reason = match ClusterRegistry::health(cx, &context) {
                ContextHealth::Failed { reason, .. } => Some(redact(&reason)),
                _ => None,
            };
            (status(cx, &context), reason)
        })
        .await
}

/// The tunnel `context` is bound to, by name and kind, read off the main
/// thread. `None` for a direct connection, or a `tunnels.toml` that can't say.
async fn bound_tunnel(path: Option<PathBuf>, context: &str) -> Option<(String, TunnelKind)> {
    let path = path.unwrap_or_else(|| crate::util::paths::preference_dir().join("tunnels.toml"));
    let context = context.to_string();
    tokio::task::spawn_blocking(move || {
        let store = TunnelStore::new(path);
        let tunnel = store.get(&store.binding_for(&context)?)?;
        Some((tunnel.name, tunnel.kind))
    })
    .await
    .ok()
    .flatten()
}

fn output(
    context: &str,
    state: ContextStatus,
    reason: Option<String>,
    already_open: bool,
) -> ToolOutput {
    let mut content = json!({
        "context": context,
        "state": state,
        "already_open": already_open,
    });
    if let (Some(reason), Value::Object(object)) = (reason, &mut content) {
        object.insert("reason".into(), Value::from(reason));
    }
    match content {
        Value::Object(object) => ToolOutput::new(object),
        _ => ToolOutput::new(Default::default()),
    }
}

#[cfg(test)]
mod tests;
