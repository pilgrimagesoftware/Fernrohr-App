//! Pod fixtures the Pods tests share.

// Named imports rather than `use super::*`: `gpui_kit::*` (imported by the
// parent) re-exports its own `test` attribute macro, which would shadow the
// built-in `#[test]` for these plain synchronous tests.
use jiff::Timestamp;
use k8s_openapi::api::core::v1::Pod;

use k8s_openapi::api::core::v1::{ContainerStatus, PodStatus};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, Time};

pub(super) fn pod(uid: &str, name: &str) -> Pod {
    pod_in("default", uid, name, 0)
}

pub(super) fn pod_in(namespace: &str, uid: &str, name: &str, created_at_secs: i64) -> Pod {
    Pod {
        metadata: ObjectMeta {
            uid: Some(uid.into()),
            name: Some(name.into()),
            namespace: Some(namespace.into()),
            creation_timestamp: Some(Time(Timestamp::from_second(created_at_secs).unwrap())),
            ..Default::default()
        },
        status: Some(PodStatus {
            phase: Some("Running".into()),
            container_statuses: Some(vec![ContainerStatus {
                name: "app".into(),
                ready: true,
                restart_count: 2,
                ..Default::default()
            }]),
            ..Default::default()
        }),
        ..Default::default()
    }
}

pub(super) fn mixed_namespace_fixture() -> Vec<Pod> {
    vec![
        pod_in("default", "u1", "web-1", 0),
        pod_in("kube-system", "u2", "coredns-1", 0),
        pod_in("kube-system", "u3", "kube-proxy-1", 0),
    ]
}
