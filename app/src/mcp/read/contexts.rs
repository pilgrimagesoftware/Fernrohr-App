//! `list_contexts` and `list_resource_kinds`: what a client can name before
//! it asks for anything else.

use super::shape::output;
use crate::mcp::cluster::{ContextStatus, kubeconfig_contexts, session, status};
use crate::mcp::kinds::kind_json;
use crate::mcp::tools::{ToolContext, ToolKind, ToolRegistry, ToolResult};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(super) fn register(registry: &mut ToolRegistry) {
    registry.add(
        "list_contexts",
        "List contexts",
        "Lists the kubeconfig contexts Fernrohr knows and whether each is open \
         and connected in the app. The other tools need a connected context.",
        ToolKind::Read,
        list_contexts,
    );
    registry.add(
        "list_resource_kinds",
        "List resource kinds",
        "Lists the resource kinds a connected context's API discovery reports, \
         with each one's group, version, plural, scope and supported verbs.",
        ToolKind::Read,
        list_resource_kinds,
    );
}

/// `list_contexts` takes no arguments.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ListContextsInput {}

async fn list_contexts(_: ListContextsInput, tools: ToolContext) -> ToolResult {
    let kubeconfig = kubeconfig_contexts(tools.kubeconfig.clone())
        .await
        .unwrap_or_default();
    let statuses: Vec<(String, ContextStatus)> = tools
        .foreground
        .run(move |cx| {
            let names: BTreeSet<String> = kubeconfig
                .into_iter()
                .chain(crate::k8s::cluster::session::ClusterRegistry::open_contexts(cx))
                .collect();
            names
                .into_iter()
                .map(|name| {
                    let status = status(cx, &name);
                    (name, status)
                })
                .collect()
        })
        .await?;
    let contexts: Vec<Value> = statuses
        .into_iter()
        .map(|(name, status)| json!({"name": name, "status": status}))
        .collect();
    Ok(output(json!({ "contexts": contexts })))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ListResourceKindsInput {
    /// The context's name, as `list_contexts` reports it.
    context: String,
}

async fn list_resource_kinds(input: ListResourceKindsInput, tools: ToolContext) -> ToolResult {
    let session = session(&tools, &input.context).await?;
    let mut kinds = session.kinds(&tools).await?;
    kinds.sort();
    let kinds: Vec<Value> = kinds.iter().map(kind_json).collect();
    Ok(output(json!({ "context": input.context, "kinds": kinds })))
}

#[cfg(test)]
mod tests;
