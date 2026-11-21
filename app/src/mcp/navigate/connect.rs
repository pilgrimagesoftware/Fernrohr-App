//! `connect_context` (`agent-mcp`: Connect context tool, Connecting requires
//! approval, Connect reports the settled state; `mcp-connections` design
//! decision 4): connects a kubeconfig context for the calling MCP session,
//! without adding it to any window, so an agent can use a cluster the user
//! doesn't want a window for.
//!
//! In order: the call must come from a live session; the name is checked
//! against the kubeconfig; then the context's current holders decide:
//!
//! | Holders now                     | Approval | Result                        |
//! |---------------------------------|----------|-------------------------------|
//! | this session already           | no       | its state                     |
//! | a user window                   | no       | the session holds it too; the frontmost window comes forward if it's one of them |
//! | only other sessions, lingering, or nobody | yes | the session holds it, connecting if needed |
//!
//! Approval is bound to the asking session (Security 1): another session's
//! hold never stands in for it. Then the tool watches the connection for a
//! bounded time and reports the first settled state, or `connecting` when the
//! wait ends; the connection carries on either way. An approved connection no
//! window holds also opens the Connections window with its row selected.

use super::super::approval::{ApprovalRequest, approve};
use super::super::cluster::{ContextStatus, caller_agent, kubeconfig_contexts, status};
use super::super::error::{ToolError, Unavailability};
use super::super::redact::redact;
use super::super::tools::{ToolContext, ToolKind, ToolOutput, ToolRegistry, ToolResult};
use crate::config::tunnels::TunnelKind;
use crate::consts::MCP_CONNECT_POLL;
use crate::k8s::cluster::context_health::ContextHealth;
use crate::k8s::cluster::session::{AgentId, ClusterRegistry, Holder};
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
        "Connects a kubeconfig context for this MCP client, without opening a window for it, \
         after the user approves it in the app. A context a Fernrohr window already has needs \
         no approval, and that window comes forward if it's the frontmost one. Waits briefly, then \
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
    let Some(agent) = caller_agent(&tools) else {
        return Err(ToolError::Unavailable {
            reason: Unavailability::NoSession,
        });
    };
    let known = kubeconfig_contexts(tools.kubeconfig.clone())
        .await
        .is_some_and(|names| names.contains(&context));
    if !known {
        return Err(ToolError::UnknownContext { context });
    }

    let name = context.clone();
    let standing = tools
        .foreground
        .run(move |cx| take_without_approval(cx, &name, agent))
        .await?;
    if standing == Standing::Ended {
        return Err(session_ended());
    }
    if standing == Standing::Held {
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

    // The session may have ended while the user was deciding; its approval
    // then grants nothing (review of App#434, F1).
    let name = context.clone();
    let held = tools
        .foreground
        .run(move |cx| {
            let held = ClusterRegistry::hold(cx, &name, agent);
            if held {
                show_in_connections(cx, &name);
            }
            held
        })
        .await?;
    if !held {
        return Err(session_ended());
    }
    let (state, reason) = settle(&tools, &context).await?;
    Ok(output(&context, state, reason, false))
}

/// The error for a call whose session ended before it could hold anything.
fn session_ended() -> ToolError {
    ToolError::Unavailable {
        reason: Unavailability::NoSession,
    }
}

/// Whether `connect_context` can go ahead without asking.
#[derive(Debug, PartialEq, Eq)]
enum Standing {
    /// The session holds the context now: it already did, or a user window
    /// does, so it took a hold of its own.
    Held,
    /// Only other sessions, lingering holds, or nobody have it: the user must
    /// approve this session.
    NeedsApproval,
    /// The session ended while the call was on its way: it can hold nothing.
    Ended,
}

/// The no-approval rows of the module's table, on the main thread.
fn take_without_approval(cx: &mut gpui_kit::App, context: &str, agent: AgentId) -> Standing {
    let holders = ClusterRegistry::holders(cx, context);
    if holders.contains(&Holder::Agent(agent)) {
        return Standing::Held;
    }
    if !holders.iter().any(Holder::is_window) {
        return Standing::NeedsApproval;
    }
    if !ClusterRegistry::hold(cx, context, agent) {
        return Standing::Ended;
    }
    shell::focus_if_held(context, cx);
    Standing::Held
}

/// A connection only agents hold has no window to show it, so the Connections
/// window opens (or comes forward) with its row selected: the user sees what
/// they approved, and can open or end it. A context a window also holds is
/// already visible there.
fn show_in_connections(cx: &mut gpui_kit::App, context: &str) {
    if ClusterRegistry::holders(cx, context)
        .iter()
        .any(Holder::is_window)
    {
        return;
    }
    if let Some(handle) = crate::ui::connections::open_or_focus(cx) {
        crate::ui::connections::select_context(context, cx);
        shell::focus_window(handle.into(), cx);
    }
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
    let agent = caller_agent(tools);
    tools
        .foreground
        .run(move |cx| {
            let reason = match ClusterRegistry::health(cx, &context) {
                ContextHealth::Failed { reason, .. } => Some(redact(&reason)),
                _ => None,
            };
            (status(cx, &context, agent), reason)
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
