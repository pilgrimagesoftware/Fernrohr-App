//! The glance projection over a crashing pod: the table's status and readiness,
//! the detail panel's owners and container states.

use super::glance;
use crate::ui::detail::BadgeTone;
use crate::ui::style::Tone;
use k8s_openapi::api::core::v1::Pod;
use serde_json::json;

/// `shop/web-1`: two containers, `app` crash-looping after 5 restarts and
/// `sidecar` running, owned by a ReplicaSet.
pub(crate) fn crashing_pod() -> Pod {
    serde_json::from_value(json!({
        "metadata": {
            "name": "web-1", "namespace": "shop", "uid": "u1",
            "creationTimestamp": "2026-10-03T00:00:00Z",
            "ownerReferences": [{
                "apiVersion": "apps/v1", "kind": "ReplicaSet", "name": "web-7d9f",
                "uid": "rs-1", "controller": true,
            }],
        },
        "spec": {
            "nodeName": "node-a",
            "containers": [
                { "name": "app", "image": "shop/web:1.4" },
                { "name": "sidecar", "image": "envoy:1.30" },
            ],
        },
        "status": {
            "phase": "Running", "podIP": "10.0.0.7",
            "containerStatuses": [
                {
                    "name": "app", "image": "shop/web:1.4", "imageID": "", "ready": false,
                    "restartCount": 5,
                    "state": { "waiting": { "reason": "CrashLoopBackOff" } },
                },
                {
                    "name": "sidecar", "image": "envoy:1.30", "imageID": "", "ready": true,
                    "restartCount": 0,
                    "state": { "running": { "startedAt": "2026-10-03T00:00:05Z" } },
                },
            ],
        },
    }))
    .expect("a valid pod")
}

#[test]
fn a_crashing_pod_at_a_glance() {
    let now: jiff::Timestamp = "2026-10-03T00:10:00Z".parse().unwrap();
    let glance = glance(&crashing_pod(), now);

    assert_eq!(
        (glance.name.as_str(), glance.namespace.as_str()),
        ("web-1", "shop")
    );
    assert_eq!(glance.status, "Running", "the phase, as the table shows it");
    assert_eq!(
        glance.status_tone,
        Tone::Bad,
        "coloured by the crash-looping container"
    );
    assert_eq!(glance.ready, "1/2");
    assert_eq!(glance.restarts, 5);
    assert_eq!(glance.age, "10m");
    assert_eq!(glance.node, "node-a");
    assert_eq!(glance.pod_ip, "10.0.0.7");
    assert_eq!(glance.owners.len(), 1);
    assert_eq!(glance.owners[0].name, "web-7d9f");

    let containers: Vec<(&str, &str, &str)> = glance
        .containers
        .iter()
        .map(|c| (c.name.as_str(), c.image.as_str(), c.state.as_str()))
        .collect();
    assert_eq!(containers[0].0, "app");
    assert_eq!(containers[0].1, "shop/web:1.4");
    assert!(
        containers[0].2.contains("CrashLoopBackOff"),
        "{containers:?}"
    );
    assert_eq!(glance.containers[0].state_tone, BadgeTone::Bad);
    assert_eq!(containers[1].0, "sidecar");
    assert!(containers[1].2.starts_with("Running"), "{containers:?}");
}
