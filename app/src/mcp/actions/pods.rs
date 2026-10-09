//! `delete_pods`: 1 to 10 Pods, named, in one namespace - irreversible, so
//! its dialog opens with focus on Cancel and lists every name it will delete.

use super::flow::{approved, kind_label, resolve};
use super::inputs::{DeletePodsInput, Target, pod_names, query};
use crate::k8s::resource::resource_actions::delete_object;
use crate::mcp::approval::{ApprovalRequest, approve};
use crate::mcp::error::ToolError;
use crate::mcp::names::ObjectName;
use crate::mcp::tools::{ToolContext, ToolKind, ToolRegistry, ToolResult};
use serde_json::{Value, json};

pub(super) fn register(registry: &mut ToolRegistry) {
    registry.add(
        "delete_pods",
        "Delete pods",
        "Deletes 1 to 10 Pods, named explicitly, in one namespace, after the \
         user allows it in Fernrohr. Owning controllers recreate them.",
        ToolKind::Action,
        delete_pods,
    );
}

async fn delete_pods(input: DeletePodsInput, tools: ToolContext) -> ToolResult {
    let names = pod_names(&input.names)?;
    // Any of the names will do for finding the session and kind.
    let target = Target::parse(&input.context, &input.namespace, names[0].as_str())?;
    let (session, kind) = resolve(&tools, &target, query("Pod", "")).await?;
    let namespace = target.namespace.as_str();
    approve(
        &tools.approvals,
        ApprovalRequest {
            title: if names.len() == 1 {
                "Delete Pod?".into()
            } else {
                format!("Delete {} Pods?", names.len())
            },
            confirm: "Delete".into(),
            tool: "delete_pods".into(),
            context: session.context.clone(),
            namespace: namespace.into(),
            kind: kind_label(&kind),
            targets: names.iter().map(|name| name.as_str().to_string()).collect(),
            parameters: vec![(
                "Effect".into(),
                "Deleted now; a controller that owns one may start a replacement".into(),
            )],
            irreversible: true,
        },
    )
    .await?;

    let mut deleted = Vec::new();
    let mut failures = Vec::new();
    for name in &names {
        match delete_object(
            session.client.clone(),
            &kind,
            name.as_str(),
            Some(namespace),
            false,
        )
        .await
        {
            Ok(()) => deleted.push(name.as_str().to_string()),
            Err(error) => failures.push(failure(name, &session.context, &error)),
        }
    }
    Ok(approved(json!({
        "context": session.context, "namespace": namespace,
        "deleted": deleted, "failed": failures,
    })))
}

fn failure(name: &ObjectName, context: &str, error: &kube::Error) -> Value {
    json!({
        "name": name.as_str(),
        "error": ToolError::from_kube(context, error).to_json(),
    })
}
