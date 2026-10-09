use super::support::{cluster, writes};
use crate::k8s::test_recorder::Recorded;
use crate::mcp::approval::scripted::{Script, Scripted};
use crate::mcp::read::test_support::call;
use crate::mcp::tools::ToolOutput;
use gpui_kit::TestAppContext;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

const MERGE: &str = "application/merge-patch+json";
const STRATEGIC: &str = "application/strategic-merge-patch+json";

/// Runs `tool` on `web` with `extra` arguments, allowed by the user, and
/// returns its result, the one question asked, and the writes sent.
async fn allowed(
    cx: &mut TestAppContext,
    tool: &str,
    extra: Value,
) -> (
    ToolOutput,
    crate::mcp::approval::ApprovalRequest,
    Vec<Recorded>,
) {
    let (tools, recorder, user): (_, _, Arc<Scripted>) =
        cluster(cx, Script::Allow, Duration::from_secs(30));
    let mut arguments = json!({"context": "dev", "namespace": "team-a", "name": "web"});
    arguments
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    if tool == "delete_pods" {
        arguments.as_object_mut().unwrap().remove("name");
    }
    let output = call(cx, &tools, tool, arguments).await.unwrap();
    let mut asked = user.asked();
    assert_eq!(asked.len(), 1, "one question");
    (output, asked.remove(0), writes(&recorder))
}

fn only(writes: &[Recorded]) -> &Recorded {
    assert_eq!(writes.len(), 1, "{writes:?}");
    &writes[0]
}

fn parameter<'a>(request: &'a crate::mcp::approval::ApprovalRequest, label: &str) -> &'a str {
    request
        .parameters
        .iter()
        .find(|(name, _)| name == label)
        .map(|(_, value)| value.as_str())
        .unwrap_or_else(|| panic!("no {label:?} in {:?}", request.parameters))
}

#[gpui_kit::test]
async fn scaling_shows_both_counts_and_patches_only_the_scale(cx: &mut TestAppContext) {
    let (output, asked, writes) = allowed(
        cx,
        "scale_workload",
        json!({"kind": "Deployment", "replicas": 3}),
    )
    .await;
    assert_eq!(asked.kind, "Deployment (apps)");
    assert_eq!(asked.targets, ["web"]);
    assert_eq!(parameter(&asked, "Replicas"), "2 → 3");
    assert!(!asked.irreversible);

    let write = only(&writes);
    assert_eq!(write.method, "PATCH");
    assert!(
        write
            .target
            .starts_with("/apis/apps/v1/namespaces/team-a/deployments/web/scale?"),
        "{}",
        write.target
    );
    assert_eq!(write.content_type.as_deref(), Some(MERGE));
    assert_eq!(write.json(), json!({"spec": {"replicas": 3}}));
    assert_eq!(output.content["outcome"], "approved");
    assert_eq!(output.content["previous_replicas"], 2);
}

#[gpui_kit::test]
async fn a_configmap_change_touches_only_its_key(cx: &mut TestAppContext) {
    let (_, asked, writes) = allowed(
        cx,
        "set_configmap_value",
        json!({"key": "LOG_LEVEL", "value": "debug"}),
    )
    .await;
    assert_eq!(parameter(&asked, "Key"), "LOG_LEVEL");
    assert_eq!(parameter(&asked, "Old value"), "\"info\"");
    assert_eq!(parameter(&asked, "New value"), "\"debug\"");

    let write = only(&writes);
    assert_eq!(write.method, "PATCH");
    assert!(
        write
            .target
            .starts_with("/api/v1/namespaces/team-a/configmaps/web?")
    );
    assert_eq!(write.content_type.as_deref(), Some(MERGE));
    // The key alone - `OTHER` isn't mentioned - and the version it was read at.
    assert_eq!(
        write.json(),
        json!({"data": {"LOG_LEVEL": "debug"}, "metadata": {"resourceVersion": "7"}})
    );
}

#[gpui_kit::test]
async fn removing_a_key_sends_null_for_it(cx: &mut TestAppContext) {
    let (_, asked, writes) = allowed(cx, "set_configmap_value", json!({"key": "LOG_LEVEL"})).await;
    assert_eq!(asked.confirm, "Remove");
    assert_eq!(parameter(&asked, "New value"), "(removed)");
    assert_eq!(
        only(&writes).json(),
        json!({"data": {"LOG_LEVEL": null}, "metadata": {"resourceVersion": "7"}})
    );
}

#[gpui_kit::test]
async fn a_restart_stamps_only_the_template_annotation(cx: &mut TestAppContext) {
    let (_, _, writes) = allowed(cx, "restart_workload", json!({"kind": "Deployment"})).await;
    let write = only(&writes);
    assert_eq!(write.method, "PATCH");
    assert!(
        write
            .target
            .starts_with("/apis/apps/v1/namespaces/team-a/deployments/web?")
    );
    let body = write.json();
    let annotations = body["spec"]["template"]["metadata"]["annotations"]
        .as_object()
        .unwrap();
    assert_eq!(annotations.len(), 1);
    let stamp = annotations["kubectl.kubernetes.io/restartedAt"]
        .as_str()
        .unwrap();
    assert!(stamp.parse::<jiff::Timestamp>().is_ok(), "{stamp}");
    assert_eq!(body.as_object().unwrap().len(), 1, "only spec: {body}");
}

#[gpui_kit::test]
async fn a_rollback_shows_its_revisions_and_restores_the_older_template(cx: &mut TestAppContext) {
    let (output, asked, writes) =
        allowed(cx, "rollback_workload", json!({"kind": "Deployment"})).await;
    assert!(asked.irreversible);
    assert_eq!(parameter(&asked, "Revision"), "2 → 1");

    let write = only(&writes);
    assert_eq!(write.method, "PATCH");
    assert_eq!(write.content_type.as_deref(), Some(STRATEGIC));
    let body = write.json();
    let template = &body["spec"]["template"];
    assert_eq!(template["$patch"], "replace");
    assert_eq!(template["spec"]["containers"][0]["image"], "web:1");
    assert_eq!(template["metadata"]["labels"], json!({"app": "web"}));
    assert_eq!(body["metadata"], json!({"resourceVersion": "7"}));
    assert_eq!(output.content["to_revision"], 1);
}

#[gpui_kit::test]
async fn pausing_sets_only_spec_paused(cx: &mut TestAppContext) {
    let (_, asked, writes) = allowed(cx, "set_rollout_paused", json!({"paused": true})).await;
    assert_eq!(parameter(&asked, "Rollout"), "running → paused");
    let write = only(&writes);
    assert!(
        write
            .target
            .starts_with("/apis/apps/v1/namespaces/team-a/deployments/web?")
    );
    assert_eq!(write.json(), json!({"spec": {"paused": true}}));
}

#[gpui_kit::test]
async fn suspending_sets_only_spec_suspend(cx: &mut TestAppContext) {
    let (_, asked, writes) = allowed(cx, "set_cronjob_suspended", json!({"suspended": true})).await;
    assert_eq!(parameter(&asked, "Schedule"), "active → suspended");
    let write = only(&writes);
    assert!(
        write
            .target
            .starts_with("/apis/batch/v1/namespaces/team-a/cronjobs/web?")
    );
    assert_eq!(write.json(), json!({"spec": {"suspend": true}}));
}

#[gpui_kit::test]
async fn a_trigger_creates_a_job_owned_by_its_cronjob(cx: &mut TestAppContext) {
    let (output, asked, writes) = allowed(cx, "trigger_cronjob", json!({})).await;
    let job_name = parameter(&asked, "New Job").to_string();
    assert!(job_name.starts_with("web-manual-"), "{job_name}");

    let write = only(&writes);
    assert_eq!(write.method, "POST");
    assert!(
        write
            .target
            .starts_with("/apis/batch/v1/namespaces/team-a/jobs?")
    );
    let job = write.json();
    assert_eq!(job["metadata"]["name"], job_name.as_str());
    assert_eq!(job["metadata"]["labels"], json!({"team": "a"}));
    assert_eq!(
        job["metadata"]["annotations"]["cronjob.kubernetes.io/instantiate"],
        "manual"
    );
    let owner = &job["metadata"]["ownerReferences"][0];
    assert_eq!(
        (owner["kind"].as_str(), owner["uid"].as_str()),
        (Some("CronJob"), Some("web-uid"))
    );
    assert_eq!(owner["controller"], true);
    assert_eq!(
        job["spec"]["template"]["spec"]["containers"][0]["image"],
        "job:1"
    );
    assert_eq!(output.content["job"], job_name.as_str());
}

#[gpui_kit::test]
async fn deleting_pods_lists_and_deletes_exactly_the_named_ones(cx: &mut TestAppContext) {
    let (output, asked, writes) =
        allowed(cx, "delete_pods", json!({"names": ["api", "web"]})).await;
    assert!(asked.irreversible);
    assert_eq!(asked.targets, ["api", "web"]);
    let targets: Vec<(&str, &str)> = writes
        .iter()
        .map(|write| (write.method.as_str(), write.target.as_str()))
        .collect();
    assert_eq!(targets.len(), 2);
    for (write, name) in targets.iter().zip(["api", "web"]) {
        assert_eq!(write.0, "DELETE");
        assert!(
            write
                .1
                .starts_with(&format!("/api/v1/namespaces/team-a/pods/{name}")),
            "{}",
            write.1
        );
    }
    assert_eq!(output.content["deleted"], json!(["api", "web"]));
}
