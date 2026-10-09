//! How a tool reaches a context: [`session`] finds the context's open,
//! connected session and hands back its client and discovery, and
//! [`status`] says where any context stands.
//!
//! Tools only use what the user already opened (`agent-mcp`: the MCP server
//! uses only Fernrohr's existing cluster sessions). A context with no session,
//! or one not connected, is an error - [`ToolError::Disconnected`] when the
//! kubeconfig has it, [`ToolError::UnknownContext`] when it doesn't - and
//! nothing here ever starts a connection.

use super::error::ToolError;
use super::kinds::{KindQuery, resolve};
use super::tools::ToolContext;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::context_health::ContextHealth;
use crate::k8s::cluster::discovery::{DiscoveredKind, discover_kinds};
use crate::k8s::cluster::discovery_registry::DiscoveryRegistry;
use crate::k8s::cluster::kubeconfig;
use crate::k8s::cluster::session::ClusterRegistry;
use gpui_kit::App;
use serde::Serialize;
use std::path::PathBuf;

/// A connected context, as one tool call uses it.
pub(super) struct Session {
    pub(super) context: String,
    pub(super) client: kube::Client,
    /// The app's discovery for the context, when it has run.
    kinds: Option<Vec<DiscoveredKind>>,
}

/// What the main thread knows about a context.
enum Lookup {
    Connected {
        client: kube::Client,
        kinds: Option<Vec<DiscoveredKind>>,
    },
    NotConnected,
    NoSession,
}

/// `context`'s session, if the user has it open and connected.
pub(super) async fn session(tools: &ToolContext, context: &str) -> Result<Session, ToolError> {
    let name = context.to_string();
    let lookup = tools.foreground.run(move |cx| lookup(cx, &name)).await?;
    match lookup {
        Lookup::Connected { client, kinds } => Ok(Session {
            context: context.to_string(),
            client,
            kinds,
        }),
        Lookup::NotConnected => Err(ToolError::Disconnected {
            context: context.to_string(),
        }),
        Lookup::NoSession => {
            let known = kubeconfig_contexts(tools.kubeconfig.clone())
                .await
                .is_some_and(|names| names.iter().any(|name| name == context));
            Err(if known {
                ToolError::Disconnected {
                    context: context.to_string(),
                }
            } else {
                ToolError::UnknownContext {
                    context: context.to_string(),
                }
            })
        }
    }
}

fn lookup(cx: &App, context: &str) -> Lookup {
    let Some(connection) = ClusterRegistry::existing_connection(cx, context) else {
        return Lookup::NoSession;
    };
    match &connection.read(cx).state {
        ConnectionState::Connected(client) => Lookup::Connected {
            client: client.clone(),
            kinds: DiscoveryRegistry::existing_kinds(cx, context),
        },
        _ => Lookup::NotConnected,
    }
}

impl Session {
    /// The context's kinds: the app's own discovery, or - when nothing in the
    /// app has asked for it yet - a fresh one, published for the next call.
    pub(super) async fn kinds(
        &self,
        tools: &ToolContext,
    ) -> Result<Vec<DiscoveredKind>, ToolError> {
        if let Some(kinds) = &self.kinds {
            return Ok(kinds.clone());
        }
        let discovered = discover_kinds(self.client.clone()).await.map_err(|error| {
            log::warn!(target: "fernrohr::mcp", "discovery failed: {error}");
            ToolError::ConnectionFailed {
                context: self.context.clone(),
            }
        })?;
        let (context, kinds) = (self.context.clone(), discovered.kinds.clone());
        // Publishing is a convenience; a client that went away meanwhile
        // still gets the kinds it asked for.
        let _ = tools
            .foreground
            .run(move |cx| DiscoveryRegistry::publish(cx, &context, kinds))
            .await;
        Ok(discovered.kinds)
    }

    /// The kind `query` names in this context, checked against its discovery
    /// before any request goes out.
    pub(super) async fn kind(
        &self,
        tools: &ToolContext,
        query: &KindQuery,
    ) -> Result<DiscoveredKind, ToolError> {
        resolve(&self.kinds(tools).await?, query)
    }
}

/// Where a context stands, as `list_contexts` reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ContextStatus {
    /// Open and connected: the tools can use it.
    Connected,
    /// Open, and connecting.
    Connecting,
    /// Open, waiting for its tunnel to come up.
    WaitingForTunnel,
    /// Open, waiting for the user to confirm its manual tunnel in the app - the
    /// agent can't answer that for them.
    AwaitingConfirmation,
    /// Open and connected, its watches paused while a tunnel reconnects or
    /// credentials refresh.
    Paused,
    /// Open, and its connection failed.
    Failed,
    /// In the kubeconfig, not open in the app.
    NotOpen,
}

/// `context`'s status. Reads app state only, so it needs the main thread.
pub(super) fn status(cx: &App, context: &str) -> ContextStatus {
    let Some(connection) = ClusterRegistry::existing_connection(cx, context) else {
        return ContextStatus::NotOpen;
    };
    match ClusterRegistry::health(cx, context) {
        ContextHealth::Paused { .. } => ContextStatus::Paused,
        ContextHealth::Failed { .. } => ContextStatus::Failed,
        ContextHealth::WaitingForTunnel { .. } => ContextStatus::WaitingForTunnel,
        ContextHealth::AwaitingConfirmation { .. } => ContextStatus::AwaitingConfirmation,
        ContextHealth::Connected => match &connection.read(cx).state {
            ConnectionState::Connected(_) => ContextStatus::Connected,
            ConnectionState::Connecting => ContextStatus::Connecting,
            ConnectionState::WaitingForTunnel => ContextStatus::WaitingForTunnel,
            ConnectionState::Failed(_) => ContextStatus::Failed,
        },
    }
}

/// The context names in `path`'s kubeconfig (`None`: the default one), read
/// off the async runtime's threads. `None` when it can't be read.
pub(super) async fn kubeconfig_contexts(path: Option<PathBuf>) -> Option<Vec<String>> {
    tokio::task::spawn_blocking(move || kubeconfig::list_context_names(path.as_deref()))
        .await
        .ok()?
        .ok()
}
