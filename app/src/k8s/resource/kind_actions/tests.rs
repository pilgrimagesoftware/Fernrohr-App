//! The kind-specific actions' rules and requests: what each kind and object is
//! offered, what an object's facts read, sizes, and - against a server that
//! records what it is sent - each call's request.

use super::{KindAction, ObjectFacts, Taint, TaintEffect, bytes};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::test_recorder::{Recorded, Recorder};
use kube::api::DynamicObject;
use kube::core::GroupVersionKind;
use serde_json::{Value, json};

fn kind(group: &str, kind: &str, plural: &str, namespaced: bool) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk(group, "v1", kind),
        plural: plural.into(),
        namespaced,
        verbs: Default::default(),
    }
}

fn object(data: Value) -> DynamicObject {
    serde_json::from_value(data).expect("an object")
}

fn facts(data: Value) -> ObjectFacts {
    ObjectFacts::of(&object(data))
}

fn offered(kind: &DiscoveredKind, data: Value) -> Vec<KindAction> {
    KindAction::offered(kind, &facts(data))
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap()
}

/// A client for a server answering with `answer`, and the server.
fn serve(
    answer: impl Fn(&Recorded) -> (&'static str, Value) + Send + Sync + 'static,
) -> (tokio::runtime::Runtime, Recorder, kube::Client) {
    let runtime = runtime();
    let (server, client) = Recorder::start_answering(runtime.handle(), answer);
    (runtime, server, client)
}

fn meta(name: &str) -> Value {
    json!({ "name": name, "namespace": "shop", "uid": format!("uid-{name}"), "resourceVersion": "7" })
}

// What's offered (tasks 2.3, 3.2, 4.2, 5.2, 6.2, 7.1).

#[test]
fn each_kind_is_offered_only_its_own_actions() {
    let cronjob = kind("batch", "CronJob", "cronjobs", true);
    assert_eq!(
        offered(&cronjob, json!({ "metadata": meta("backup"), "spec": {} })),
        [KindAction::TriggerNow, KindAction::Suspend]
    );
    assert_eq!(
        offered(
            &cronjob,
            json!({ "metadata": meta("backup"), "spec": { "suspend": true } })
        ),
        [KindAction::TriggerNow, KindAction::Resume],
        "a suspended CronJob offers Resume instead"
    );
    let job = kind("batch", "Job", "jobs", true);
    assert_eq!(
        offered(&job, json!({ "metadata": meta("once"), "spec": {} })),
        [KindAction::Suspend]
    );
    let deployment = kind("apps", "Deployment", "deployments", true);
    assert_eq!(
        offered(
            &deployment,
            json!({ "metadata": meta("web"), "spec": { "paused": true } })
        ),
        [KindAction::ResumeRollout]
    );
    for other in [
        kind("apps", "DaemonSet", "daemonsets", true),
        kind("apps", "StatefulSet", "statefulsets", true),
        kind("", "ConfigMap", "configmaps", true),
    ] {
        assert!(
            offered(
                &other,
                json!({ "metadata": meta("x"), "spec": { "paused": false } })
            )
            .is_empty(),
            "{} has no kind actions",
            other.gvk.kind
        );
    }
    assert_eq!(
        offered(
            &kind("", "Service", "services", true),
            json!({ "metadata": meta("web") })
        ),
        [KindAction::ForwardService]
    );
    assert_eq!(
        offered(
            &kind("", "ServiceAccount", "serviceaccounts", true),
            json!({ "metadata": meta("deploy") })
        ),
        [KindAction::CreateToken]
    );
}

#[test]
fn a_claim_without_a_size_or_an_ingress_without_a_host_offers_nothing() {
    let pvc = kind("", "PersistentVolumeClaim", "persistentvolumeclaims", true);
    assert!(offered(&pvc, json!({ "metadata": meta("data"), "spec": {} })).is_empty());
    assert_eq!(
        offered(
            &pvc,
            json!({ "metadata": meta("data"),
                "spec": { "resources": { "requests": { "storage": "10Gi" } } } })
        ),
        [KindAction::Expand]
    );
    let ingress = kind("networking.k8s.io", "Ingress", "ingresses", true);
    assert!(offered(&ingress, json!({ "metadata": meta("web"), "spec": {} })).is_empty());
}

#[test]
fn a_node_offers_remove_taint_only_with_taints() {
    let node = kind("", "Node", "nodes", false);
    assert_eq!(
        offered(&node, json!({ "metadata": meta("node-a"), "spec": {} })),
        [KindAction::AddTaint]
    );
    assert_eq!(
        offered(
            &node,
            json!({ "metadata": meta("node-a"),
                "spec": { "taints": [{ "key": "dedicated", "effect": "NoSchedule" }] } })
        ),
        [KindAction::AddTaint, KindAction::RemoveTaint]
    );
}

#[test]
fn a_kind_that_cant_be_patched_is_offered_no_mutation() {
    let mut cronjob = kind("batch", "CronJob", "cronjobs", true);
    cronjob.verbs.patch = false;
    assert_eq!(
        offered(&cronjob, json!({ "metadata": meta("backup"), "spec": {} })),
        [KindAction::TriggerNow]
    );
}

#[test]
fn an_ingress_offers_its_first_rules_host() {
    let facts = facts(json!({ "metadata": meta("web"),
        "spec": { "rules": [{ "host": "app.example.com" }, { "host": "api.example.com" }] } }));
    assert_eq!(facts.host.as_deref(), Some("app.example.com"));
}

#[test]
fn sizes_read_as_bytes() {
    assert_eq!(bytes("10Gi"), Some(10.0 * 1_073_741_824.0));
    assert_eq!(bytes("500M"), Some(5e8));
    assert_eq!(bytes("1.5Ti"), Some(1.5 * 1_099_511_627_776.0));
    assert_eq!(bytes("2e9"), Some(2e9));
    assert_eq!(bytes("1000"), Some(1000.0));
    for bad in ["", "Gi", "ten", "10GB", "-1Gi", "1..5Gi"] {
        assert_eq!(bytes(bad), None, "{bad:?}");
    }
    assert!(bytes("20Gi") > bytes("10Gi"));
    assert!(bytes("10G") < bytes("10Gi"));
}

#[test]
fn a_taint_reads_and_parses_its_effect() {
    assert_eq!("noschedule".parse(), Ok(TaintEffect::NoSchedule));
    assert!("Never".parse::<TaintEffect>().is_err());
    let taint = Taint::new("dedicated", Some("gpu"), TaintEffect::NoSchedule);
    assert_eq!(taint.label(), "dedicated=gpu:NoSchedule");
    assert_eq!(
        taint.raw,
        json!({ "key": "dedicated", "value": "gpu", "effect": "NoSchedule" })
    );
}

// Requests (tasks 2.1, 3.1, 4.1, 5.1, 6.1).

#[test]
fn suspend_and_pause_patch_one_field_only_on_the_listed_object() {
    let (runtime, server, client) = serve(|_| ("200 OK", json!({ "metadata": { "name": "x" } })));
    let cronjob = kind("batch", "CronJob", "cronjobs", true);
    let deployment = kind("apps", "Deployment", "deployments", true);
    runtime
        .block_on(super::set_suspended(
            client.clone(),
            &cronjob,
            "shop",
            "backup",
            true,
            Some("uid-backup"),
        ))
        .expect("suspended");
    runtime
        .block_on(super::set_paused(
            client,
            &deployment,
            "shop",
            "web",
            true,
            Some("uid-web"),
        ))
        .expect("paused");
    let requests = server.requests();
    assert_eq!(
        requests[0].json(),
        json!({ "metadata": { "uid": "uid-backup" }, "spec": { "suspend": true } })
    );
    assert!(
        requests[0]
            .target
            .starts_with("/apis/batch/v1/namespaces/shop/cronjobs/backup")
    );
    assert_eq!(
        requests[1].json(),
        json!({ "metadata": { "uid": "uid-web" }, "spec": { "paused": true } })
    );
}

#[test]
fn a_patch_on_a_recreated_object_says_so() {
    let (runtime, _server, client) = serve(|_| {
        (
            "409 Conflict",
            json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure",
                "reason": "Conflict", "code": 409,
                "message": "Precondition failed: UID in precondition: uid-web, UID in object meta: other" }),
        )
    });
    let failure = runtime
        .block_on(super::set_paused(
            client,
            &kind("apps", "Deployment", "deployments", true),
            "shop",
            "web",
            true,
            Some("uid-web"),
        ))
        .unwrap_err();
    assert!(failure.message.starts_with("Not changed:"), "{failure:?}");
}

#[test]
fn expand_patches_the_requested_storage_and_reports_the_clusters_refusal() {
    let pvc = kind("", "PersistentVolumeClaim", "persistentvolumeclaims", true);
    let (runtime, server, client) =
        serve(|_| ("200 OK", json!({ "metadata": { "name": "data" } })));
    runtime
        .block_on(super::expand(
            client,
            &pvc,
            "shop",
            "data",
            "20Gi",
            Some("uid-data"),
        ))
        .expect("expanded");
    assert_eq!(
        server.requests()[0].json(),
        json!({ "metadata": { "uid": "uid-data" },
            "spec": { "resources": { "requests": { "storage": "20Gi" } } } })
    );

    let refusal = "persistentvolumeclaims \"data\" is forbidden: only dynamically provisioned pvc \
                   can be resized and the storageclass that provisions the pvc must support resize";
    let (runtime, _server, client) = serve(move |_| {
        (
            "403 Forbidden",
            json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure",
                "reason": "Forbidden", "code": 403, "message": refusal }),
        )
    });
    let failure = runtime
        .block_on(super::expand(client, &pvc, "shop", "data", "20Gi", None))
        .unwrap_err();
    assert!(
        failure.message.contains("must support resize")
            || failure.detail.contains("must support resize"),
        "the cluster's own words: {failure:?}"
    );
}

fn node_facts() -> ObjectFacts {
    facts(json!({ "metadata": meta("node-a"), "spec": { "taints": [
        { "key": "dedicated", "value": "gpu", "effect": "NoSchedule" },
        { "key": "node.kubernetes.io/unreachable", "effect": "NoExecute",
          "timeAdded": "2026-10-14T12:00:00Z" },
    ] } }))
}

#[test]
fn add_taint_writes_every_taint_for_the_version_it_read() {
    let node = kind("", "Node", "nodes", false);
    let facts = node_facts();
    let taint = Taint::new("team", None, TaintEffect::PreferNoSchedule);
    let (runtime, server, client) =
        serve(|_| ("200 OK", json!({ "metadata": { "name": "node-a" } })));
    runtime
        .block_on(super::add_taint(client, &node, "node-a", &facts, &taint))
        .expect("added");
    let sent = server.requests()[0].json();
    assert_eq!(
        sent["metadata"],
        json!({ "uid": "uid-node-a", "resourceVersion": "7" })
    );
    let taints = sent["spec"]["taints"].as_array().expect("a list");
    assert_eq!(taints.len(), 3);
    assert_eq!(
        taints[2],
        json!({ "key": "team", "effect": "PreferNoSchedule" })
    );
}

#[test]
fn remove_taint_drops_only_that_taint_and_keeps_the_others_as_they_were() {
    let node = kind("", "Node", "nodes", false);
    let facts = node_facts();
    let gone = facts.taints[0].clone();
    let (runtime, server, client) =
        serve(|_| ("200 OK", json!({ "metadata": { "name": "node-a" } })));
    runtime
        .block_on(super::remove_taint(client, &node, "node-a", &facts, &gone))
        .expect("removed");
    assert_eq!(
        server.requests()[0].json()["spec"]["taints"],
        json!([{ "key": "node.kubernetes.io/unreachable", "effect": "NoExecute",
            "timeAdded": "2026-10-14T12:00:00Z" }]),
        "the other taint, timeAdded and all"
    );
}

#[test]
fn trigger_now_creates_a_job_from_the_template_and_leaves_the_cronjob_alone() {
    let (requests, job) = {
        let (runtime, server, client) = serve(|request| {
            if request.method == "GET" {
                (
                    "200 OK",
                    json!({ "apiVersion": "batch/v1", "kind": "CronJob", "metadata": meta("backup"),
                        "spec": { "schedule": "0 3 * * *", "jobTemplate": {
                            "metadata": { "labels": { "app": "backup" } },
                            "spec": { "template": { "spec": { "restartPolicy": "Never",
                                "containers": [{ "name": "backup", "image": "backup:1" }] } } } } } }),
                )
            } else {
                (
                    "201 Created",
                    json!({ "apiVersion": "batch/v1", "kind": "Job", "metadata": { "name": "x" } }),
                )
            }
        });
        let job = runtime.block_on(super::trigger_now(client, "shop", "backup"));
        (server.requests(), job)
    };
    let job = job.expect("triggered");
    assert!(job.starts_with("backup-manual-"), "{job}");
    let created: Vec<&Recorded> = requests.iter().filter(|r| r.method == "POST").collect();
    assert_eq!(created.len(), 1);
    assert!(
        created[0]
            .target
            .starts_with("/apis/batch/v1/namespaces/shop/jobs")
    );
    let sent = created[0].json();
    assert_eq!(
        sent["spec"]["template"]["spec"]["containers"][0]["image"],
        json!("backup:1")
    );
    assert_eq!(
        sent["metadata"]["ownerReferences"][0]["uid"],
        json!("uid-backup")
    );
    assert!(
        !requests
            .iter()
            .any(|r| r.method == "PATCH" || r.method == "PUT"),
        "the CronJob itself is never written"
    );
}

#[test]
fn create_token_sends_a_token_request_and_holds_the_answer_as_a_secret() {
    let (runtime, server, client) = serve(|_| {
        (
            "201 Created",
            json!({ "apiVersion": "authentication.k8s.io/v1", "kind": "TokenRequest",
                "status": { "token": "eyJhbGciOiJSUzI1NiJ9.secret-token", "expirationTimestamp": "2026-10-14T13:00:00Z" } }),
        )
    });
    let token = runtime
        .block_on(super::create_token(client, "shop", "deploy"))
        .expect("a token");
    let requests = server.requests();
    assert_eq!(requests[0].method, "POST");
    assert!(
        requests[0]
            .target
            .starts_with("/api/v1/namespaces/shop/serviceaccounts/deploy/token"),
        "{}",
        requests[0].target
    );
    assert_eq!(
        requests[0].json(),
        json!({ "apiVersion": "authentication.k8s.io/v1", "kind": "TokenRequest", "spec": {} })
    );
    assert_eq!(token.expose(), Some("eyJhbGciOiJSUzI1NiJ9.secret-token"));
    let printed = format!("{token:?} {token}");
    assert!(
        !printed.contains("secret-token"),
        "it never prints: {printed}"
    );
}

#[test]
fn an_answer_without_a_token_says_so_and_carries_none_of_it() {
    let (runtime, _server, client) = serve(|_| {
        (
            "201 Created",
            json!({ "kind": "TokenRequest", "status": { "token": "" } }),
        )
    });
    let failure = runtime
        .block_on(super::create_token(client, "shop", "deploy"))
        .unwrap_err();
    assert_eq!(failure.message, "The cluster's answer held no token.");
    assert!(failure.detail.is_empty());
}
