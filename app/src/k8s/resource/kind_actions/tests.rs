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

/// A server holding Node `node-a` as it is now - its UID `uid`, version "9",
/// and two taints, one with a `timeAdded` - answering a get with it and a patch
/// with OK.
fn node_server(uid: &'static str) -> (tokio::runtime::Runtime, Recorder, kube::Client) {
    serve(move |request| {
        if request.method == "GET" {
            (
                "200 OK",
                json!({ "apiVersion": "v1", "kind": "Node",
                    "metadata": { "name": "node-a", "uid": uid, "resourceVersion": "9" },
                    "spec": { "taints": [
                        { "key": "dedicated", "value": "gpu", "effect": "NoSchedule" },
                        { "key": "node.kubernetes.io/unreachable", "effect": "NoExecute",
                          "timeAdded": "2026-10-14T12:00:00Z" },
                    ] } }),
            )
        } else {
            ("200 OK", json!({ "metadata": { "name": "node-a" } }))
        }
    })
}

fn patches(server: &Recorder) -> Vec<Value> {
    server
        .requests()
        .iter()
        .filter(|request| request.method == "PATCH")
        .map(Recorded::json)
        .collect()
}

/// Review fix 2: Add Taint reads the node when it runs, and writes every taint
/// it has then - not the list the dialog opened on - pinned to that version.
#[test]
fn add_taint_writes_every_taint_the_node_has_now_for_that_version() {
    let node = kind("", "Node", "nodes", false);
    let taint = Taint::new("team", None, TaintEffect::PreferNoSchedule);
    let (runtime, server, client) = node_server("uid-node-a");
    runtime
        .block_on(super::add_taint(
            client,
            &node,
            "node-a",
            Some("uid-node-a"),
            &taint,
        ))
        .expect("added");
    let sent = patches(&server).remove(0);
    assert_eq!(
        sent["metadata"],
        json!({ "uid": "uid-node-a", "resourceVersion": "9" }),
        "the version just read"
    );
    let taints = sent["spec"]["taints"].as_array().expect("a list");
    assert_eq!(taints.len(), 3);
    assert_eq!(
        taints[2],
        json!({ "key": "team", "effect": "PreferNoSchedule" })
    );
}

#[test]
fn add_taint_refuses_a_taint_the_node_already_has() {
    let node = kind("", "Node", "nodes", false);
    let taint = Taint::new("dedicated", Some("cpu"), TaintEffect::NoSchedule);
    let (runtime, server, client) = node_server("uid-node-a");
    let failure = runtime
        .block_on(super::add_taint(client, &node, "node-a", None, &taint))
        .unwrap_err();
    assert!(failure.message.contains("already has"), "{failure:?}");
    assert!(patches(&server).is_empty());
}

#[test]
fn remove_taint_drops_only_that_taint_and_keeps_the_others_as_they_were() {
    let node = kind("", "Node", "nodes", false);
    let gone = Taint::new("dedicated", Some("gpu"), TaintEffect::NoSchedule);
    let (runtime, server, client) = node_server("uid-node-a");
    runtime
        .block_on(super::remove_taint(client, &node, "node-a", None, &gone))
        .expect("removed");
    assert_eq!(
        patches(&server)[0]["spec"]["taints"],
        json!([{ "key": "node.kubernetes.io/unreachable", "effect": "NoExecute",
            "timeAdded": "2026-10-14T12:00:00Z" }]),
        "the other taint, timeAdded and all"
    );
}

#[test]
fn a_taint_already_gone_needs_no_patch() {
    let node = kind("", "Node", "nodes", false);
    let gone = Taint::new("team", None, TaintEffect::NoSchedule);
    let (runtime, server, client) = node_server("uid-node-a");
    runtime
        .block_on(super::remove_taint(client, &node, "node-a", None, &gone))
        .expect("nothing to do");
    assert!(patches(&server).is_empty());
}

/// A node recreated under the same name since it was listed isn't changed.
#[test]
fn a_recreated_node_isnt_tainted() {
    let node = kind("", "Node", "nodes", false);
    let taint = Taint::new("team", None, TaintEffect::NoSchedule);
    let (runtime, server, client) = node_server("uid-other");
    let failure = runtime
        .block_on(super::add_taint(
            client,
            &node,
            "node-a",
            Some("uid-node-a"),
            &taint,
        ))
        .unwrap_err();
    assert_eq!(
        failure.message,
        crate::k8s::resource::resource_actions::RECREATED
    );
    assert!(patches(&server).is_empty());
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
        let job = runtime.block_on(super::trigger_now(
            client,
            "shop",
            "backup",
            Some("uid-backup"),
        ));
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

/// Review fix 3: Trigger Now refuses a CronJob recreated under the same name.
#[test]
fn trigger_now_refuses_a_recreated_cronjob() {
    let (runtime, server, client) = serve(|_| {
        (
            "200 OK",
            json!({ "apiVersion": "batch/v1", "kind": "CronJob",
                "metadata": { "name": "backup", "namespace": "shop", "uid": "uid-other" },
                "spec": { "schedule": "0 3 * * *", "jobTemplate": { "spec": {
                    "template": { "spec": { "containers": [] } } } } } }),
        )
    });
    let failure = runtime
        .block_on(super::trigger_now(
            client,
            "shop",
            "backup",
            Some("uid-backup"),
        ))
        .unwrap_err();
    assert_eq!(
        failure.message,
        crate::k8s::resource::resource_actions::RECREATED
    );
    assert!(
        !server
            .requests()
            .iter()
            .any(|request| request.method == "POST"),
        "no Job is created"
    );
}

/// Review fix 1: only an openable host is offered - the first rule's that is -
/// and a wildcard is never one.
#[test]
fn open_host_offers_the_first_openable_rule_host() {
    let ingress =
        |rules: Value| facts(json!({ "metadata": meta("web"), "spec": { "rules": rules } }));
    assert_eq!(
        ingress(json!([{ "http": {} }, { "host": "app.example.com" }]))
            .host
            .as_deref(),
        Some("app.example.com"),
        "the first rule that has a host"
    );
    assert_eq!(
        ingress(json!([{ "host": "*.example.com" }, { "host": "api.example.com" }]))
            .host
            .as_deref(),
        Some("api.example.com"),
        "a wildcard is passed over"
    );
    assert_eq!(ingress(json!([{ "host": "*.example.com" }])).host, None);
    let ingresses = kind("networking.k8s.io", "Ingress", "ingresses", true);
    assert!(
        offered(
            &ingresses,
            json!({ "metadata": meta("web"),
            "spec": { "rules": [{ "host": "*.example.com" }] } })
        )
        .is_empty(),
        "nothing to open"
    );
}

#[test]
fn an_openable_host_is_a_lowercase_dns_name() {
    use super::openable;
    for good in ["app.example.com", "a", "x-1.example.com", "localhost"] {
        assert!(openable(good), "{good}");
    }
    let long = format!("{}.example.com", "a".repeat(64));
    for bad in [
        "",
        "*.example.com",
        ".example.com",
        "example.com.",
        "App.example.com",
        "a..b",
        "-a.example.com",
        "a-.example.com",
        "app.example.com/path",
        "user@app.example.com",
        "app.example.com:8443",
        long.as_str(),
    ] {
        assert!(!openable(bad), "{bad:?}");
    }
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
