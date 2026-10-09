//! `scale_workload`, `restart_workload`, `rollback_workload` and
//! `set_rollout_paused`: the workload actions, each through `flow`'s steps.

use super::flow::{approved, failed, kind_label, resolve};
use super::inputs::{
    RolloutInput, ScaleWorkloadInput, SetRolloutPausedInput, Target, apps, replicas,
};
use crate::k8s::resource::resource_actions as actions;
use crate::mcp::approval::{ApprovalRequest, Asking, approve};
use crate::mcp::tools::{ToolContext, ToolKind, ToolRegistry, ToolResult};
use serde_json::json;

pub(super) fn register(registry: &mut ToolRegistry) {
    registry.add(
        "scale_workload",
        "Scale workload",
        "Sets a Deployment's, StatefulSet's or ReplicaSet's replica count \
         through its scale subresource, after the user allows it in Fernrohr.",
        ToolKind::Action,
        scale_workload,
    );
    registry.add(
        "restart_workload",
        "Restart workload",
        "Restarts a Deployment's, StatefulSet's or DaemonSet's pods by a \
         rolling update, as `kubectl rollout restart` does, after the user \
         allows it in Fernrohr.",
        ToolKind::Action,
        restart_workload,
    );
    registry.add(
        "rollback_workload",
        "Roll back workload",
        "Rolls a Deployment, StatefulSet or DaemonSet back to its previous \
         revision, as `kubectl rollout undo` does, after the user allows it \
         in Fernrohr.",
        ToolKind::Action,
        rollback_workload,
    );
    registry.add(
        "set_rollout_paused",
        "Pause or resume rollout",
        "Pauses or resumes a Deployment's rollout by setting `spec.paused`, \
         after the user allows it in Fernrohr.",
        ToolKind::Action,
        set_rollout_paused,
    );
}

async fn scale_workload(input: ScaleWorkloadInput, tools: ToolContext) -> ToolResult {
    let target = Target::parse(&input.context, &input.namespace, &input.name)?;
    let requested = replicas(input.replicas)?;
    let (session, kind) = resolve(&tools, &target, apps(input.kind.name())).await?;
    let (namespace, name) = (target.namespace.as_str(), target.name.as_str());
    let current = actions::current_replicas(session.client.clone(), &kind, namespace, name)
        .await
        .map_err(failed(&session))?;
    approve(
        &tools.approvals,
        ApprovalRequest {
            title: format!("Scale {}?", kind.gvk.kind),
            confirm: "Scale".into(),
            tool: "scale_workload".into(),
            context: session.context.clone(),
            namespace: namespace.into(),
            kind: kind_label(&kind),
            targets: vec![name.into()],
            parameters: vec![("Replicas".into(), format!("{current} → {requested}"))],
            asking: Asking::Action {
                irreversible: false,
            },
        },
    )
    .await?;
    let replicas = actions::scale(session.client.clone(), &kind, namespace, name, requested)
        .await
        .map_err(failed(&session))?;
    Ok(approved(json!({
        "context": session.context, "namespace": namespace,
        "kind": kind.gvk.kind, "name": name,
        "previous_replicas": current, "replicas": replicas,
    })))
}

async fn restart_workload(input: RolloutInput, tools: ToolContext) -> ToolResult {
    let target = Target::parse(&input.context, &input.namespace, &input.name)?;
    let (session, kind) = resolve(&tools, &target, apps(input.kind.name())).await?;
    let (namespace, name) = (target.namespace.as_str(), target.name.as_str());
    approve(
        &tools.approvals,
        ApprovalRequest {
            title: format!("Restart {}?", kind.gvk.kind),
            confirm: "Restart".into(),
            tool: "restart_workload".into(),
            context: session.context.clone(),
            namespace: namespace.into(),
            kind: kind_label(&kind),
            targets: vec![name.into()],
            parameters: vec![(
                "Effect".into(),
                "Replaces every pod by a rolling update".into(),
            )],
            asking: Asking::Action {
                irreversible: false,
            },
        },
    )
    .await?;
    let at = jiff::Timestamp::now();
    actions::restart(session.client.clone(), &kind, namespace, name, at)
        .await
        .map_err(failed(&session))?;
    Ok(approved(json!({
        "context": session.context, "namespace": namespace,
        "kind": kind.gvk.kind, "name": name, "restarted_at": at.to_string(),
    })))
}

async fn rollback_workload(input: RolloutInput, tools: ToolContext) -> ToolResult {
    let target = Target::parse(&input.context, &input.namespace, &input.name)?;
    let (session, kind) = resolve(&tools, &target, apps(input.kind.name())).await?;
    let (namespace, name) = (target.namespace.as_str(), target.name.as_str());
    let plan = actions::plan_rollback(session.client.clone(), &kind, namespace, name)
        .await
        .map_err(failed(&session))?;
    approve(
        &tools.approvals,
        ApprovalRequest {
            title: format!("Roll back {}?", kind.gvk.kind),
            confirm: "Roll Back".into(),
            tool: "rollback_workload".into(),
            context: session.context.clone(),
            namespace: namespace.into(),
            kind: kind_label(&kind),
            targets: vec![name.into()],
            parameters: vec![(
                "Revision".into(),
                format!("{} → {}", plan.current_revision, plan.target_revision),
            )],
            asking: Asking::Action { irreversible: true },
        },
    )
    .await?;
    actions::rollback(session.client.clone(), &kind, namespace, name, &plan)
        .await
        .map_err(failed(&session))?;
    Ok(approved(json!({
        "context": session.context, "namespace": namespace,
        "kind": kind.gvk.kind, "name": name,
        "from_revision": plan.current_revision, "to_revision": plan.target_revision,
    })))
}

async fn set_rollout_paused(input: SetRolloutPausedInput, tools: ToolContext) -> ToolResult {
    let target = Target::parse(&input.context, &input.namespace, &input.name)?;
    let (session, kind) = resolve(&tools, &target, apps("Deployment")).await?;
    let (namespace, name) = (target.namespace.as_str(), target.name.as_str());
    let paused = actions::deployment_paused(session.client.clone(), &kind, namespace, name)
        .await
        .map_err(failed(&session))?;
    let state = |paused: bool| if paused { "paused" } else { "running" };
    approve(
        &tools.approvals,
        ApprovalRequest {
            title: format!(
                "{} Deployment rollout?",
                if input.paused { "Pause" } else { "Resume" }
            ),
            confirm: if input.paused { "Pause" } else { "Resume" }.into(),
            tool: "set_rollout_paused".into(),
            context: session.context.clone(),
            namespace: namespace.into(),
            kind: kind_label(&kind),
            targets: vec![name.into()],
            parameters: vec![(
                "Rollout".into(),
                format!("{} → {}", state(paused), state(input.paused)),
            )],
            asking: Asking::Action {
                irreversible: false,
            },
        },
    )
    .await?;
    actions::set_rollout_paused(session.client.clone(), &kind, namespace, name, input.paused)
        .await
        .map_err(failed(&session))?;
    Ok(approved(json!({
        "context": session.context, "namespace": namespace,
        "kind": kind.gvk.kind, "name": name, "paused": input.paused,
    })))
}
