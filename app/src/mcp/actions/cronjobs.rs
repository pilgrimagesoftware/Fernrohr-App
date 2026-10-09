//! `trigger_cronjob` and `set_cronjob_suspended`.

use super::flow::{approved, failed, kind_label, resolve};
use super::inputs::{CronJobInput, SetCronJobSuspendedInput, Target, query};
use crate::k8s::resource::resource_actions as actions;
use crate::mcp::approval::{ApprovalRequest, approve};
use crate::mcp::error::ToolError;
use crate::mcp::tools::{ToolContext, ToolKind, ToolRegistry, ToolResult};
use serde_json::json;

pub(super) fn register(registry: &mut ToolRegistry) {
    registry.add(
        "trigger_cronjob",
        "Trigger CronJob",
        "Creates a Job from a CronJob's template now, as `kubectl create job \
         --from=cronjob/<name>` does, after the user allows it in Fernrohr.",
        ToolKind::Action,
        trigger_cronjob,
    );
    registry.add(
        "set_cronjob_suspended",
        "Suspend or resume CronJob",
        "Suspends or resumes a CronJob's schedule by setting `spec.suspend`, \
         after the user allows it in Fernrohr.",
        ToolKind::Action,
        set_cronjob_suspended,
    );
}

async fn trigger_cronjob(input: CronJobInput, tools: ToolContext) -> ToolResult {
    let target = Target::parse(&input.context, &input.namespace, &input.name)?;
    let (session, kind) = resolve(&tools, &target, query("CronJob", "batch")).await?;
    let (namespace, name) = (target.namespace.as_str(), target.name.as_str());
    let plan = actions::plan_trigger(session.client.clone(), namespace, name, &job_suffix()?)
        .await
        .map_err(failed(&session))?;
    approve(
        &tools.approvals,
        ApprovalRequest {
            title: "Trigger CronJob?".into(),
            confirm: "Trigger".into(),
            tool: "trigger_cronjob".into(),
            context: session.context.clone(),
            namespace: namespace.into(),
            kind: kind_label(&kind),
            targets: vec![name.into()],
            parameters: vec![("New Job".into(), plan.job_name.clone())],
            irreversible: false,
        },
    )
    .await?;
    actions::trigger(session.client.clone(), namespace, &plan)
        .await
        .map_err(failed(&session))?;
    Ok(approved(json!({
        "context": session.context, "namespace": namespace,
        "cronjob": name, "job": plan.job_name,
    })))
}

async fn set_cronjob_suspended(input: SetCronJobSuspendedInput, tools: ToolContext) -> ToolResult {
    let target = Target::parse(&input.context, &input.namespace, &input.name)?;
    let (session, kind) = resolve(&tools, &target, query("CronJob", "batch")).await?;
    let (namespace, name) = (target.namespace.as_str(), target.name.as_str());
    let suspended = actions::cronjob_suspended(session.client.clone(), &kind, namespace, name)
        .await
        .map_err(failed(&session))?;
    let state = |suspended: bool| if suspended { "suspended" } else { "active" };
    let verb = if input.suspended { "Suspend" } else { "Resume" };
    approve(
        &tools.approvals,
        ApprovalRequest {
            title: format!("{verb} CronJob?"),
            confirm: verb.into(),
            tool: "set_cronjob_suspended".into(),
            context: session.context.clone(),
            namespace: namespace.into(),
            kind: kind_label(&kind),
            targets: vec![name.into()],
            parameters: vec![(
                "Schedule".into(),
                format!("{} → {}", state(suspended), state(input.suspended)),
            )],
            irreversible: false,
        },
    )
    .await?;
    actions::set_cronjob_suspended(
        session.client.clone(),
        &kind,
        namespace,
        name,
        input.suspended,
    )
    .await
    .map_err(failed(&session))?;
    Ok(approved(json!({
        "context": session.context, "namespace": namespace,
        "name": name, "suspended": input.suspended,
    })))
}

/// Five random lowercase letters and digits, for a manual Job's name - as
/// `kubectl` and k9s suffix theirs.
fn job_suffix() -> Result<String, ToolError> {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut bytes = [0u8; 5];
    getrandom::fill(&mut bytes).map_err(|_| ToolError::Internal)?;
    Ok(bytes
        .iter()
        .map(|byte| ALPHABET[usize::from(*byte) % ALPHABET.len()] as char)
        .collect())
}
