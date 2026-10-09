//! `set_configmap_value`: one key of one ConfigMap's `data`, set or removed.
//! The tool takes no kind, so it can't be pointed at a Secret.

use super::flow::{approved, failed, kind_label, preview, resolve};
use super::inputs::{SetConfigMapValueInput, Target, configmap_key, query};
use crate::k8s::resource::resource_actions as actions;
use crate::mcp::approval::{ApprovalRequest, approve};
use crate::mcp::error::ToolError;
use crate::mcp::tools::{ToolContext, ToolKind, ToolRegistry, ToolResult};
use serde_json::json;

pub(super) fn register(registry: &mut ToolRegistry) {
    registry.add(
        "set_configmap_value",
        "Set ConfigMap value",
        "Sets one key in a ConfigMap's `data`, or removes it when `value` is \
         omitted, after the user allows it in Fernrohr. No other key or field \
         changes.",
        ToolKind::Action,
        set_configmap_value,
    );
}

async fn set_configmap_value(input: SetConfigMapValueInput, tools: ToolContext) -> ToolResult {
    let target = Target::parse(&input.context, &input.namespace, &input.name)?;
    let key = configmap_key(&input.key)?;
    let (session, kind) = resolve(&tools, &target, query("ConfigMap", "")).await?;
    let (namespace, name) = (target.namespace.as_str(), target.name.as_str());
    let read = actions::configmap_value(session.client.clone(), &kind, namespace, name, &key)
        .await
        .map_err(failed(&session))?;
    if input.value.is_none() && read.value.is_none() {
        return Err(ToolError::Precondition {
            message: format!("ConfigMap {name:?} has no key {key:?} to remove"),
        });
    }
    approve(
        &tools.approvals,
        ApprovalRequest {
            title: if input.value.is_some() {
                "Set ConfigMap value?".into()
            } else {
                "Remove ConfigMap key?".into()
            },
            confirm: if input.value.is_some() {
                "Set"
            } else {
                "Remove"
            }
            .into(),
            tool: "set_configmap_value".into(),
            context: session.context.clone(),
            namespace: namespace.into(),
            kind: kind_label(&kind),
            targets: vec![name.into()],
            parameters: vec![
                ("Key".into(), key.clone()),
                (
                    "Old value".into(),
                    preview(read.value.as_deref(), "(not set)"),
                ),
                (
                    "New value".into(),
                    preview(input.value.as_deref(), "(removed)"),
                ),
            ],
            irreversible: false,
        },
    )
    .await?;
    actions::set_configmap_value(
        session.client.clone(),
        &kind,
        namespace,
        name,
        &key,
        input.value.as_deref(),
        &read,
    )
    .await
    .map_err(failed(&session))?;
    Ok(approved(json!({
        "context": session.context, "namespace": namespace, "name": name,
        "key": key, "removed": input.value.is_none(),
    })))
}
