//! The allowlisted actions through the adapter, against section 3's fixture
//! cluster: an allowed action writes only its own change, and one denied or
//! left unanswered writes nothing.

use super::{Agent, content, error_code};
use crate::mcp::actions::tests::support::{cluster, every_action, writes};
use crate::mcp::approval::scripted::Script;
use gpui_kit::TestAppContext;
use serde_json::json;
use std::time::Duration;

/// Longer than any test here waits, so only the script answers.
const PATIENT: Duration = Duration::from_secs(30);

#[gpui_kit::test]
async fn an_approved_action_reports_approval_and_writes_only_its_change(cx: &mut TestAppContext) {
    let (tools, recorder, user) = cluster(cx, Script::Allow, PATIENT);
    let mut agent = Agent::connect(cx, tools).await;

    let result = agent
        .call(
            cx,
            "scale_workload",
            json!({"context": "dev", "namespace": "team-a", "name": "web",
                   "kind": "Deployment", "replicas": 3}),
        )
        .await;

    let output = content(&result);
    assert_eq!(output["outcome"], "approved", "{result}");
    let asked = user.asked();
    assert_eq!(asked.len(), 1, "one question");
    assert_eq!(asked[0].tool, "scale_workload");
    assert_eq!(asked[0].targets, ["web"]);
    assert!(
        asked[0]
            .parameters
            .contains(&("Replicas".to_string(), "2 → 3".to_string())),
        "{:?}",
        asked[0].parameters
    );
    let writes = writes(&recorder);
    assert_eq!(writes.len(), 1, "{writes:?}");
    assert_eq!(writes[0].method, "PATCH");
    assert!(
        writes[0]
            .target
            .starts_with("/apis/apps/v1/namespaces/team-a/deployments/web/scale?"),
        "{}",
        writes[0].target
    );
    assert_eq!(writes[0].json(), json!({"spec": {"replicas": 3}}));
}

#[gpui_kit::test]
async fn a_denied_action_is_reported_and_writes_nothing(cx: &mut TestAppContext) {
    let (tools, recorder, user) = cluster(cx, Script::Deny, PATIENT);
    let mut agent = Agent::connect(cx, tools).await;

    for (tool, arguments) in every_action() {
        let result = agent.call(cx, tool, arguments).await;
        assert_eq!(error_code(&result), "denied", "{tool}");
    }

    assert_eq!(
        user.asked().len(),
        every_action().len(),
        "each one was asked"
    );
    assert!(writes(&recorder).is_empty(), "{:?}", writes(&recorder));
}

#[gpui_kit::test]
async fn an_unanswered_action_times_out_and_writes_nothing(cx: &mut TestAppContext) {
    let (tools, recorder, user) = cluster(cx, Script::Never, Duration::from_millis(50));
    let mut agent = Agent::connect(cx, tools).await;

    let result = agent
        .call(
            cx,
            "delete_pods",
            json!({"context": "dev", "namespace": "team-a", "names": ["api"]}),
        )
        .await;

    assert_eq!(error_code(&result), "approval_timed_out");
    assert_eq!(user.withdrawn(), 1, "the question was withdrawn");
    assert!(writes(&recorder).is_empty(), "{:?}", writes(&recorder));
}

#[gpui_kit::test]
async fn an_action_outside_its_allowlist_is_refused_before_anyone_is_asked(
    cx: &mut TestAppContext,
) {
    let (tools, recorder, user) = cluster(cx, Script::Allow, PATIENT);
    let mut agent = Agent::connect(cx, tools).await;

    let result = agent
        .call(
            cx,
            "scale_workload",
            json!({"context": "dev", "namespace": "team-a", "name": "web",
                   "kind": "DaemonSet", "replicas": 3}),
        )
        .await;

    assert_eq!(error_code(&result), "invalid_arguments", "{result}");
    assert!(user.asked().is_empty(), "nobody was asked");
    assert!(writes(&recorder).is_empty());
}
