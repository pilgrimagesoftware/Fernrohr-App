//! The delete helper's requests, against a server that records them: a normal
//! delete and a force kill differ only in the grace period they ask for.

use super::{ActionFailure, delete};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::test_recorder::Recorder;
use serde_json::json;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap()
}

fn deleted() -> serde_json::Value {
    json!({ "kind": "Status", "apiVersion": "v1", "status": "Success", "code": 200 })
}

#[test]
fn a_delete_asks_for_the_default_grace_period() {
    let runtime = runtime();
    let (server, client) = Recorder::start(runtime.handle(), "200 OK", deleted());

    runtime
        .block_on(delete(
            client,
            &DiscoveredKind::pods(),
            "web-1",
            Some("shop"),
            false,
        ))
        .expect("deleted");

    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "DELETE");
    assert!(
        requests[0]
            .target
            .starts_with("/api/v1/namespaces/shop/pods/web-1"),
        "{}",
        requests[0].target
    );
    assert_eq!(
        requests[0].json().get("gracePeriodSeconds"),
        None,
        "the pod's own grace period applies"
    );
}

#[test]
fn a_force_kill_asks_for_no_grace_period() {
    let runtime = runtime();
    let (server, client) = Recorder::start(runtime.handle(), "200 OK", deleted());

    runtime
        .block_on(delete(
            client,
            &DiscoveredKind::pods(),
            "web-1",
            Some("shop"),
            true,
        ))
        .expect("deleted");

    let requests = server.requests();
    assert_eq!(requests[0].method, "DELETE");
    assert_eq!(requests[0].json()["gracePeriodSeconds"], json!(0));
}

#[test]
fn a_rejected_delete_reports_why() {
    let runtime = runtime();
    let forbidden = json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure",
        "reason": "Forbidden", "code": 403,
        "message": "pods \"web-1\" is forbidden: User \"dev\" cannot delete resource \"pods\"" });
    let (_server, client) = Recorder::start(runtime.handle(), "403 Forbidden", forbidden);

    let failure: ActionFailure = runtime
        .block_on(delete(
            client,
            &DiscoveredKind::pods(),
            "web-1",
            Some("shop"),
            false,
        ))
        .expect_err("refused");

    assert!(failure.message.starts_with("Forbidden: "), "{failure:?}");
}

#[test]
fn deleting_what_is_already_gone_succeeds() {
    let runtime = runtime();
    let gone = json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure",
        "reason": "NotFound", "code": 404, "message": "pods \"web-1\" not found" });
    let (_server, client) = Recorder::start(runtime.handle(), "404 Not Found", gone);

    runtime
        .block_on(delete(
            client,
            &DiscoveredKind::pods(),
            "web-1",
            Some("shop"),
            false,
        ))
        .expect("already gone is done");
}
