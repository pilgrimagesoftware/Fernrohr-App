use super::support::{cluster, every_action, writes};
use crate::mcp::actions::ALLOWLIST;
use crate::mcp::approval::scripted::Script;
use crate::mcp::error::ToolError;
use crate::mcp::read::test_support::call;
use crate::mcp::tools::ToolRegistry;
use gpui_kit::TestAppContext;
use serde_json::json;
use std::time::Duration;

const LONG: Duration = Duration::from_secs(30);

#[test]
fn every_allowlisted_action_is_exercised_here() {
    let mut tested: Vec<&str> = every_action().into_iter().map(|(name, _)| name).collect();
    tested.sort();
    assert_eq!(tested, ALLOWLIST);
}

#[gpui_kit::test]
async fn out_of_scope_requests_are_refused_before_anyone_is_asked(cx: &mut TestAppContext) {
    let (tools, recorder, user) = cluster(cx, Script::Allow, LONG);
    let eleven: Vec<String> = (0..11).map(|n| format!("pod-{n}")).collect();
    let cases = [
        // A kind outside the tool's own allowlist.
        (
            "scale_workload",
            json!({"kind": "DaemonSet", "replicas": 1}),
        ),
        (
            "set_rollout_paused",
            json!({"kind": "StatefulSet", "paused": true}),
        ),
        (
            "set_configmap_value",
            json!({"kind": "Secret", "key": "password", "value": "x"}),
        ),
        ("restart_workload", json!({"kind": "ReplicaSet"})),
        // Over the cap, or a name that would reshape the request path.
        ("delete_pods", json!({"names": eleven})),
        ("delete_pods", json!({"names": ["../secrets/db"]})),
        (
            "scale_workload",
            json!({"kind": "Deployment", "replicas": 1, "name": "x?dryRun=All"}),
        ),
        ("set_configmap_value", json!({"key": "a/b", "value": "x"})),
        // Not a context, or not connected.
        (
            "restart_workload",
            json!({"kind": "Deployment", "context": "prod"}),
        ),
    ];
    for (tool, overrides) in cases {
        let mut arguments = json!({"context": "dev", "namespace": "team-a", "name": "web"});
        if tool == "delete_pods" {
            arguments.as_object_mut().unwrap().remove("name");
        }
        arguments
            .as_object_mut()
            .unwrap()
            .extend(overrides.as_object().unwrap().clone());
        let result = call(cx, &tools, tool, arguments.clone()).await;
        let error = result.expect_err(&arguments.to_string());
        assert!(
            matches!(
                error,
                ToolError::InvalidArguments { .. } | ToolError::UnknownContext { .. }
            ),
            "{tool} {arguments}: {error:?}"
        );
    }
    assert!(user.asked().is_empty(), "{:?}", user.asked());
    assert!(recorder.requests().is_empty(), "{:?}", recorder.requests());
}

#[gpui_kit::test]
async fn a_kind_the_context_lacks_is_refused_before_anyone_is_asked(cx: &mut TestAppContext) {
    let (tools, recorder, user) = cluster(cx, Script::Allow, LONG);
    // Discovery here has no CronJob - an older cluster, say.
    cx.update(|cx| {
        crate::k8s::cluster::discovery_registry::DiscoveryRegistry::insert_test(
            cx,
            "dev",
            vec![crate::mcp::test_support::discovered(
                "", "v1", "Pod", "pods", true,
            )],
        );
    });
    let result = call(
        cx,
        &tools,
        "trigger_cronjob",
        json!({"context": "dev", "namespace": "team-a", "name": "web"}),
    )
    .await;
    assert_eq!(
        result,
        Err(ToolError::UnsupportedKind {
            kind: "CronJob".into()
        })
    );
    assert!(user.asked().is_empty());
    assert!(recorder.requests().is_empty());
}

#[gpui_kit::test]
async fn a_denied_action_writes_nothing(cx: &mut TestAppContext) {
    let (tools, recorder, user) = cluster(cx, Script::Deny, LONG);
    for (tool, arguments) in every_action() {
        let result = call(cx, &tools, tool, arguments).await;
        assert_eq!(result, Err(ToolError::Denied), "{tool}");
    }
    let asked: Vec<String> = user
        .asked()
        .into_iter()
        .map(|request| request.tool)
        .collect();
    let expected: Vec<&str> = every_action().into_iter().map(|(name, _)| name).collect();
    assert_eq!(asked, expected, "each action asked once");
    assert!(writes(&recorder).is_empty(), "{:?}", writes(&recorder));
}

#[gpui_kit::test]
async fn an_unanswered_action_times_out_and_writes_nothing(cx: &mut TestAppContext) {
    let (tools, recorder, user) = cluster(cx, Script::Never, Duration::from_millis(50));
    for (tool, arguments) in every_action() {
        let result = call(cx, &tools, tool, arguments).await;
        assert_eq!(result, Err(ToolError::ApprovalTimedOut), "{tool}");
    }
    assert_eq!(
        user.withdrawn(),
        every_action().len(),
        "each question withdrawn"
    );
    assert!(writes(&recorder).is_empty());
}

#[gpui_kit::test]
async fn a_client_that_hangs_up_withdraws_its_question_and_writes_nothing(cx: &mut TestAppContext) {
    let (tools, recorder, user) = cluster(cx, Script::Never, LONG);
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let arguments = crate::mcp::test_support::object(
        json!({"context": "dev", "namespace": "team-a", "names": ["api"]}),
    );
    let pending = handle.spawn(ToolRegistry::app().call("delete_pods", arguments, tools.clone()));
    // Wait until the question is up, then hang up - the endpoint drops the
    // call's future when its client goes, as aborting does here.
    let watching = user.clone();
    handle
        .spawn(async move {
            while watching.asked().is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    assert_eq!(user.asked().len(), 1);
    pending.abort();
    let _ = pending.await;
    assert_eq!(user.withdrawn(), 1);
    assert!(writes(&recorder).is_empty());
}

#[gpui_kit::test]
async fn removing_a_key_that_isnt_there_asks_nothing(cx: &mut TestAppContext) {
    let (tools, recorder, user) = cluster(cx, Script::Allow, LONG);
    let result = call(
        cx,
        &tools,
        "set_configmap_value",
        json!({"context": "dev", "namespace": "team-a", "name": "web", "key": "MISSING"}),
    )
    .await;
    assert!(
        matches!(result, Err(ToolError::Precondition { .. })),
        "{result:?}"
    );
    assert!(user.asked().is_empty());
    assert!(writes(&recorder).is_empty());
}

#[gpui_kit::test]
async fn an_upstream_auth_failure_reaches_the_agent_redacted(cx: &mut TestAppContext) {
    let user = crate::mcp::approval::scripted::Scripted::new(Script::Allow);
    let tools = crate::mcp::tools::ToolContext {
        approvals: user.gate(),
        ..crate::mcp::read::test_support::tools(cx, &["dev"])
    };
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let refusal = json!({
        "kind": "Status", "apiVersion": "v1", "status": "Failure", "reason": "Unauthorized",
        "code": 401, "message": "invalid bearer token: Authorization: Bearer s3cr3t-t0k3n",
    });
    let (_recorder, client) =
        crate::k8s::test_recorder::Recorder::start(&handle, "401 Unauthorized", refusal);
    crate::mcp::read::test_support::open(
        cx,
        "dev",
        crate::k8s::cluster::connection::ConnectionState::Connected(client),
        Some(vec![crate::mcp::test_support::discovered(
            "apps",
            "v1",
            "Deployment",
            "deployments",
            true,
        )]),
    );
    let result = call(
        cx,
        &tools,
        "scale_workload",
        json!({"context": "dev", "namespace": "team-a", "name": "web",
               "kind": "Deployment", "replicas": 3}),
    )
    .await;
    let error = result.expect_err("a 401 fails the action");
    assert_eq!(error.code(), "kubernetes");
    let shown = error.to_json().to_string();
    assert!(!shown.contains("s3cr3t-t0k3n"), "{shown}");
    assert!(user.asked().is_empty(), "a failed read asks nothing");
}
