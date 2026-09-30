//! The fetch against a fixture API server (`resource-links` 5.2): a namespaced
//! object and its events, a cluster-scoped object whose events are listed
//! cluster-wide, events the user may not list, a 404, and a Secret arriving
//! already redacted.

use super::fixtures::{Route, kind, nodes, owned_replica_set, replica_sets, serve, target};
use crate::k8s::resource::object_detail::fetch::{ObjectFetch, fetch_object};
use crate::ui::nav::ObjectTarget;
use serde_json::json;

fn event_list(reason: &str) -> serde_json::Value {
    json!({
        "apiVersion": "v1",
        "kind": "EventList",
        "metadata": {},
        "items": [{
            "metadata": { "name": "e.1", "namespace": "default" },
            "involvedObject": { "kind": "Any", "name": "any" },
            "reason": reason,
            "type": "Normal",
        }],
    })
}

/// Runs `fetch_object(target)` against a server serving `routes`.
fn fetch(routes: Vec<Route>, target: ObjectTarget) -> Result<ObjectFetch, (String, String)> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (addr, _server) = runtime.block_on(serve(routes));
    runtime.block_on(async {
        let client =
            kube::Client::try_from(kube::Config::new(format!("http://{addr}").parse().unwrap()))
                .expect("build client");
        fetch_object(client, target).await
    })
}

#[test]
fn a_namespaced_object_loads_with_its_events() {
    let result = fetch(
        vec![
            Route {
                path: "/api/v1/namespaces/staging/events?",
                status: "200 OK",
                body: event_list("ScalingReplicaSet"),
            },
            Route {
                path: "/apis/apps/v1/namespaces/staging/replicasets/web-7d9f ",
                status: "200 OK",
                body: serde_json::to_value(owned_replica_set()).unwrap(),
            },
        ],
        target(replica_sets(), Some("staging"), "web-7d9f"),
    );

    let Ok(ObjectFetch::Found(object, events)) = result else {
        panic!("the server serves this ReplicaSet");
    };
    assert_eq!(object.metadata.name.as_deref(), Some("web-7d9f"));
    let events = events.expect("its events are listable");
    assert_eq!(events[0].reason.as_deref(), Some("ScalingReplicaSet"));
}

/// A cluster-scoped object is read without a namespace, and its events are
/// listed across all namespaces.
#[test]
fn a_cluster_scoped_object_loads_with_cluster_wide_events() {
    let result = fetch(
        vec![
            Route {
                path: "/api/v1/events?",
                status: "200 OK",
                body: event_list("NodeReady"),
            },
            Route {
                path: "/api/v1/nodes/node-a ",
                status: "200 OK",
                body: json!({
                    "apiVersion": "v1",
                    "kind": "Node",
                    "metadata": { "name": "node-a" },
                }),
            },
        ],
        target(nodes(), None, "node-a"),
    );

    let Ok(ObjectFetch::Found(object, events)) = result else {
        panic!("the server serves this Node");
    };
    assert_eq!(object.metadata.name.as_deref(), Some("node-a"));
    assert_eq!(
        events.expect("listed")[0].reason.as_deref(),
        Some("NodeReady")
    );
}

/// An object the user may read loads even when its events can't be listed;
/// the events failure travels beside it rather than failing the fetch.
#[test]
fn forbidden_events_do_not_fail_the_object() {
    let result = fetch(
        vec![
            Route {
                path: "/events?",
                status: "403 Forbidden",
                body: json!({
                    "kind": "Status",
                    "apiVersion": "v1",
                    "status": "Failure",
                    "message": "events is forbidden",
                    "reason": "Forbidden",
                    "code": 403,
                }),
            },
            Route {
                path: "/api/v1/nodes/node-a ",
                status: "200 OK",
                body: json!({ "apiVersion": "v1", "kind": "Node", "metadata": { "name": "node-a" } }),
            },
        ],
        target(nodes(), None, "node-a"),
    );

    let Ok(ObjectFetch::Found(_, events)) = result else {
        panic!("the Node itself is readable");
    };
    assert!(events.is_err(), "the 403 is reported, not an empty list");
}

#[test]
fn a_missing_object_is_not_found_rather_than_a_failure() {
    let result = fetch(
        Vec::new(),
        target(kind("", "v1", "ConfigMap", true), Some("staging"), "gone"),
    );
    assert!(matches!(result, Ok(ObjectFetch::NotFound)));
}

/// The fetch hands back a Secret with its values already replaced.
#[test]
fn a_secret_arrives_redacted() {
    let result = fetch(
        vec![Route {
            path: "/api/v1/namespaces/staging/secrets/app-tls ",
            status: "200 OK",
            body: json!({
                "apiVersion": "v1",
                "kind": "Secret",
                "type": "kubernetes.io/tls",
                "metadata": { "name": "app-tls", "namespace": "staging" },
                "data": { "tls.key": "c2VjcmV0LWtleQ==" },
            }),
        }],
        target(kind("", "v1", "Secret", true), Some("staging"), "app-tls"),
    );

    let Ok(ObjectFetch::Found(object, _)) = result else {
        panic!("the server serves this Secret");
    };
    let rendered = serde_json::to_string(&object).unwrap();
    assert!(
        !rendered.contains("c2VjcmV0LWtleQ=="),
        "the encoded value is gone"
    );
    assert_eq!(object.data["data"]["tls.key"], "<redacted: 10 bytes>");
}
