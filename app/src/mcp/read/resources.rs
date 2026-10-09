//! `list_resources` and `get_resource`: objects of any discovered kind, read
//! through the context's own session.
//!
//! The kind is resolved against discovery, the namespace checked against the
//! kind's scope and every name parsed (`names`) before a request is built, so
//! a kind the context lacks, a misplaced namespace or a path-altering name
//! never reaches the cluster.

use super::shape::{object_json, output, within_budget};
use crate::consts::{MCP_LIST_DEFAULT_LIMIT, MCP_LIST_MAX_LIMIT, MCP_RESULT_BUDGET_BYTES};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::resource_actions::api_for;
use crate::mcp::cluster::session;
use crate::mcp::error::ToolError;
use crate::mcp::kinds::{KindQuery, kind_json};
use crate::mcp::names::{LabelName, ObjectName};
use crate::mcp::tools::{ToolContext, ToolKind, ToolRegistry, ToolResult};
use kube::api::ListParams;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn register(registry: &mut ToolRegistry) {
    registry.add(
        "list_resources",
        "List resources",
        "Lists objects of one resource kind in a connected context: in one \
         namespace, or in all of them when `namespace` is omitted. Pages with \
         `limit` and the `continue` token a previous page returned. A page \
         too large to return whole is cut short and marked `truncated`, with \
         no `continue` token: ask again with a smaller `limit`. Secret values \
         are replaced by their sizes.",
        ToolKind::Read,
        list_resources,
    );
    registry.add(
        "get_resource",
        "Get resource",
        "Returns one object's current representation from a connected \
         context. Secret values are replaced by their sizes.",
        ToolKind::Read,
        get_resource,
    );
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ListResourcesInput {
    /// The context's name, as `list_contexts` reports it.
    context: String,
    /// The resource kind: its name (`Deployment`), plural (`deployments`)
    /// or singular (`deployment`), as `list_resource_kinds` reports it.
    kind: String,
    /// The kind's API group, needed only when two groups have a kind of
    /// that name: `apps`, or `core` for the core group.
    #[serde(default)]
    group: Option<String>,
    /// The namespace to list; omit for every namespace. Only for
    /// namespaced kinds.
    #[serde(default)]
    namespace: Option<String>,
    /// A label selector, as `kubectl get -l` takes it (`app=web,tier!=db`).
    #[serde(default)]
    label_selector: Option<String>,
    /// A field selector, as `kubectl get --field-selector` takes it
    /// (`status.phase=Running`).
    #[serde(default)]
    field_selector: Option<String>,
    /// The most objects to return in one page, 1 to 500; 100 when omitted.
    #[serde(default)]
    limit: Option<u32>,
    /// The `continue` token of the previous page, to fetch the next one.
    #[serde(default, rename = "continue")]
    continue_token: Option<String>,
}

async fn list_resources(input: ListResourcesInput, tools: ToolContext) -> ToolResult {
    let limit = input.limit.unwrap_or(MCP_LIST_DEFAULT_LIMIT);
    if !(1..=MCP_LIST_MAX_LIMIT).contains(&limit) {
        return Err(ToolError::InvalidArguments {
            message: format!("`limit` must be between 1 and {MCP_LIST_MAX_LIMIT}"),
        });
    }
    let namespace = optional_label("namespace", input.namespace.as_deref())?;
    let session = session(&tools, &input.context).await?;
    let kind = session
        .kind(&tools, &kind_query(&input.kind, input.group))
        .await?;
    if !kind.verbs.list {
        return Err(ToolError::UnsupportedOperation {
            kind: kind.gvk.kind.clone(),
            operation: "list".to_string(),
        });
    }
    if namespace.is_some() && !kind.namespaced {
        return Err(cluster_scoped(&kind));
    }

    let mut params = ListParams::default().limit(limit);
    params.label_selector = input.label_selector;
    params.field_selector = input.field_selector;
    params.continue_token = input.continue_token;
    let api = api_for(
        session.client.clone(),
        &kind,
        namespace.as_ref().map(LabelName::as_str),
    );
    let list = api
        .list(&params)
        .await
        .map_err(|error| ToolError::from_kube(&session.context, &error))?;

    let items = list
        .items
        .into_iter()
        .map(|object| object_json(&kind, object))
        .collect::<Result<Vec<Value>, ToolError>>()?;
    let page_size = items.len();
    let (items, truncated) = within_budget(items, MCP_RESULT_BUDGET_BYTES);
    // The token continues after the whole page; after a cut one it would
    // skip the objects left out.
    let next = list
        .metadata
        .continue_
        .filter(|token| !token.is_empty() && !truncated);
    let mut result = output(json!({
        "context": session.context,
        "kind": kind_json(&kind),
        "namespace": namespace.as_ref().map(LabelName::as_str),
        "count": items.len(),
        "page_size": page_size,
        "items": items,
        "continue": next,
    }));
    result.truncated = truncated;
    Ok(result)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct GetResourceInput {
    /// The context's name, as `list_contexts` reports it.
    context: String,
    /// The resource kind: its name, plural or singular.
    kind: String,
    /// The kind's API group, needed only when two groups have a kind of
    /// that name: `apps`, or `core` for the core group.
    #[serde(default)]
    group: Option<String>,
    /// The object's namespace: required for a namespaced kind, and absent
    /// for a cluster-scoped one.
    #[serde(default)]
    namespace: Option<String>,
    /// The object's name.
    name: String,
}

async fn get_resource(input: GetResourceInput, tools: ToolContext) -> ToolResult {
    let name = ObjectName::parse("name", &input.name)?;
    let namespace = optional_label("namespace", input.namespace.as_deref())?;
    let session = session(&tools, &input.context).await?;
    let kind = session
        .kind(&tools, &kind_query(&input.kind, input.group))
        .await?;
    match (&namespace, kind.namespaced) {
        (Some(_), false) => return Err(cluster_scoped(&kind)),
        (None, true) => {
            return Err(ToolError::InvalidArguments {
                message: format!("{} is namespaced; pass `namespace`", kind.gvk.kind),
            });
        }
        _ => {}
    }

    let api = api_for(
        session.client.clone(),
        &kind,
        namespace.as_ref().map(LabelName::as_str),
    );
    let object = api
        .get(name.as_str())
        .await
        .map_err(|error| ToolError::from_kube(&session.context, &error))?;
    let object = object_json(&kind, object)?;
    let size = serde_json::to_vec(&object).map_or(usize::MAX, |bytes| bytes.len());
    if size > MCP_RESULT_BUDGET_BYTES {
        return Err(ToolError::ResultTooLarge {
            limit: MCP_RESULT_BUDGET_BYTES,
        });
    }
    Ok(output(json!({
        "context": session.context,
        "object": object,
    })))
}

fn kind_query(kind: &str, group: Option<String>) -> KindQuery {
    KindQuery {
        kind: kind.to_string(),
        group,
    }
}

fn optional_label(field: &str, value: Option<&str>) -> Result<Option<LabelName>, ToolError> {
    value
        .map(|value| LabelName::parse(field, value))
        .transpose()
}

fn cluster_scoped(kind: &DiscoveredKind) -> ToolError {
    ToolError::InvalidArguments {
        message: format!("{} is cluster-scoped; omit `namespace`", kind.gvk.kind),
    }
}

#[cfg(test)]
mod tests;
