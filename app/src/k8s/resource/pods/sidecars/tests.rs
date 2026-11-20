//! #353: sidecars - init containers with `restartPolicy: Always` - counted
//! with the app containers in a pod's Ready and Restarts, and listed apart
//! from both app and run-once init containers.

use super::{ready_text, sidecar_statuses};
use crate::k8s::resource::pod_detail::glance::glance;
use crate::k8s::resource::pods::pod_row;
use jiff::Timestamp;
use k8s_openapi::api::core::v1::Pod;
use serde_json::json;

/// An app container (ready), a sidecar (not ready, restarted twice) and a
/// run-once init container that has finished.
pub(crate) fn pod() -> Pod {
    serde_json::from_value(json!({
        "metadata": { "name": "web-1", "namespace": "shop" },
        "spec": {
            "containers": [{ "name": "app", "image": "shop/web:1" }],
            "initContainers": [
                { "name": "migrate", "image": "shop/migrate:1" },
                { "name": "proxy", "image": "mesh/proxy:2", "restartPolicy": "Always" },
            ],
        },
        "status": {
            "phase": "Running",
            "containerStatuses": [
                { "name": "app", "image": "shop/web:1", "imageID": "", "ready": true,
                  "restartCount": 1, "state": { "running": {} } },
            ],
            "initContainerStatuses": [
                { "name": "migrate", "image": "shop/migrate:1", "imageID": "", "ready": false,
                  "restartCount": 0,
                  "state": { "terminated": { "exitCode": 0, "reason": "Completed" } } },
                { "name": "proxy", "image": "mesh/proxy:2", "imageID": "", "ready": false,
                  "restartCount": 2, "state": { "running": {} } },
            ],
        },
    }))
    .expect("a valid pod")
}

#[test]
fn ready_text_names_the_sidecars() {
    assert_eq!(ready_text(1, 1, 0), "1/1");
    assert_eq!(ready_text(2, 2, 1), "2/2 (1 sidecar)");
    assert_eq!(ready_text(1, 3, 2), "1/3 (2 sidecars)");
}

/// Only the restartable init container is a sidecar.
#[test]
fn only_always_restarting_init_containers_are_sidecars() {
    let pod = pod();
    let names: Vec<&str> = sidecar_statuses(&pod)
        .iter()
        .map(|status| status.name.as_str())
        .collect();
    assert_eq!(names, ["proxy"]);
}

/// The Pods row counts the sidecar like `kubectl` - so a not-ready sidecar
/// keeps the pod short of ready - and says it's there; the finished init
/// container counts for neither.
#[test]
fn the_pods_row_counts_sidecars() {
    let row = pod_row(&pod(), Timestamp::now());
    assert_eq!(row.ready, "1/2 (1 sidecar)");
    assert_eq!(row.restarts, 3, "the app's 1 and the sidecar's 2");
}

/// The quick look shows the sidecar after the app container, marked.
#[test]
fn the_quick_look_marks_the_sidecar() {
    let glance = glance(&pod(), Timestamp::now());
    let containers: Vec<(&str, bool)> = glance
        .containers
        .iter()
        .map(|c| (c.name.as_str(), c.sidecar))
        .collect();
    assert_eq!(containers, [("app", false), ("proxy", true)]);
}

/// #398: the pickers offer the app containers, then the sidecars - never a
/// run-once init container - and say which are sidecars.
#[test]
fn pickers_offer_app_containers_then_sidecars() {
    let pod = pod();
    let (containers, sidecars) = super::long_running_names(&pod);
    assert_eq!(containers, ["app", "proxy"]);
    assert_eq!(sidecars, ["proxy"]);

    let selection = crate::k8s::resource::pods::PodSelection::of(&pod, "ctx");
    assert_eq!(selection.containers, ["app", "proxy"]);
    assert_eq!(selection.container_label("proxy"), "proxy (sidecar)");
    assert_eq!(selection.container_label("app"), "app");
}

/// #398: a running sidecar can be shelled into; the finished init container
/// can't.
#[test]
fn a_running_sidecar_is_a_shell_target() {
    assert_eq!(
        crate::k8s::resource::pods::running_containers(&pod()),
        ["app", "proxy"]
    );
}
