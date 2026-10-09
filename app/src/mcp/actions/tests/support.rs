use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::test_recorder::{Recorded, Recorder};
use crate::mcp::approval::scripted::{Script, Scripted};
use crate::mcp::read::test_support::{open, tools};
use crate::mcp::test_support::discovered;
use crate::mcp::tools::ToolContext;
use gpui_kit::TestAppContext;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

/// What the fake cluster answers to every request: one object `web` in
/// `team-a` that reads as each kind the actions read - a Deployment at
/// revision 2 with 2 replicas, a ConfigMap with `LOG_LEVEL: info`, a CronJob,
/// a Scale - and as a list holding the Deployment's revision-1 ReplicaSet.
pub(super) fn every_answer() -> Value {
    let template = json!({
        "metadata": {"labels": {"app": "web", "pod-template-hash": "abc12"}},
        "spec": {"containers": [{"name": "web", "image": "web:1"}]},
    });
    json!({
        "apiVersion": "v1", "kind": "Thing",
        "metadata": {
            "name": "web", "namespace": "team-a", "uid": "web-uid", "resourceVersion": "7",
            "annotations": {"deployment.kubernetes.io/revision": "2"},
        },
        "spec": {
            "replicas": 2, "paused": false, "suspend": false,
            "schedule": "0 * * * *",
            "template": {"spec": {"containers": []}},
            "jobTemplate": {
                "metadata": {"labels": {"team": "a"}},
                "spec": {"template": {"spec": {"containers": [{"name": "job", "image": "job:1"}]}}},
            },
        },
        "data": {"LOG_LEVEL": "info", "OTHER": "kept"},
        "items": [{
            "metadata": {
                "name": "web-abc12", "uid": "rs-uid",
                "annotations": {"deployment.kubernetes.io/revision": "1"},
                "ownerReferences": [
                    {"apiVersion": "apps/v1", "kind": "Deployment", "name": "web", "uid": "web-uid"}
                ],
            },
            "spec": {"selector": {}, "template": template},
        }],
    })
}

/// [`every_answer`], typed where the client reads a typed object: those
/// check their `apiVersion` and `kind`.
fn answer(request: &Recorded) -> Value {
    let mut body = every_answer();
    let path = request.target.split('?').next().unwrap_or_default();
    let typed = if path.ends_with("/scale") {
        Some(("autoscaling/v1", "Scale"))
    } else if path.contains("/cronjobs/") {
        Some(("batch/v1", "CronJob"))
    } else if path.ends_with("/jobs") {
        Some(("batch/v1", "Job"))
    } else {
        None
    };
    if let Some((api_version, kind)) = typed {
        body["apiVersion"] = json!(api_version);
        body["kind"] = json!(kind);
    }
    body
}

fn kinds() -> Vec<DiscoveredKind> {
    vec![
        discovered("", "v1", "Pod", "pods", true),
        discovered("", "v1", "ConfigMap", "configmaps", true),
        discovered("", "v1", "Secret", "secrets", true),
        discovered("apps", "v1", "Deployment", "deployments", true),
        discovered("apps", "v1", "StatefulSet", "statefulsets", true),
        discovered("apps", "v1", "DaemonSet", "daemonsets", true),
        discovered("apps", "v1", "ReplicaSet", "replicasets", true),
        discovered("batch", "v1", "CronJob", "cronjobs", true),
    ]
}

/// A connected `dev` context on a recording cluster answering
/// [`every_answer`], whose user answers by `script` within `timeout`.
pub(super) fn cluster(
    cx: &mut TestAppContext,
    script: Script,
    timeout: Duration,
) -> (ToolContext, Recorder, Arc<Scripted>) {
    let user = Scripted::new(script);
    let tools = ToolContext {
        approvals: user.gate_with_timeout(timeout),
        ..tools(cx, &["dev"])
    };
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let (recorder, client) =
        Recorder::start_answering(&handle, |request| ("200 OK", answer(request)));
    open(cx, "dev", ConnectionState::Connected(client), Some(kinds()));
    (tools, recorder, user)
}

/// The requests that could change something: everything but reads.
pub(super) fn writes(recorder: &Recorder) -> Vec<Recorded> {
    recorder
        .requests()
        .into_iter()
        .filter(|request| request.method != "GET")
        .collect()
}

/// Each action tool, called on `web` in `team-a`.
pub(super) fn every_action() -> Vec<(&'static str, Value)> {
    let on_web = |extra: Value| {
        let mut arguments = json!({"context": "dev", "namespace": "team-a", "name": "web"});
        arguments
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        arguments
    };
    vec![
        (
            "set_configmap_value",
            on_web(json!({"key": "LOG_LEVEL", "value": "debug"})),
        ),
        (
            "scale_workload",
            on_web(json!({"kind": "Deployment", "replicas": 3})),
        ),
        ("restart_workload", on_web(json!({"kind": "Deployment"}))),
        ("rollback_workload", on_web(json!({"kind": "Deployment"}))),
        ("set_rollout_paused", on_web(json!({"paused": true}))),
        (
            "delete_pods",
            json!({"context": "dev", "namespace": "team-a", "names": ["api", "web"]}),
        ),
        ("trigger_cronjob", on_web(json!({}))),
        ("set_cronjob_suspended", on_web(json!({"suspended": true}))),
    ]
}
