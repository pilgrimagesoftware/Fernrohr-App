//! Shared fixtures: a pod carrying every field the projection reads, and small
//! lookup helpers.

use crate::k8s::resource::pod_detail::model::PodField;
use jiff::Timestamp;
use k8s_openapi::api::core::v1::{
    Container, HostIP, Pod, PodCondition, PodIP, PodSpec, PodStatus, Toleration,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{
    ManagedFieldsEntry, ObjectMeta, OwnerReference, Time,
};

pub(super) fn field<'a>(fields: &'a [PodField], label: &str) -> Option<&'a PodField> {
    fields.iter().find(|field| field.label == label)
}

/// A pod carrying every field the projection reads, so a test can assert
/// the whole list rather than each field in isolation.
pub(super) fn rich_pod() -> Pod {
    Pod {
        metadata: ObjectMeta {
            name: Some("api-7d9f-ftg5t".into()),
            namespace: Some("staging".into()),
            creation_timestamp: Some(Time(Timestamp::from_second(0).unwrap())),
            labels: Some(
                [
                    ("app".to_string(), "api".to_string()),
                    ("tier".to_string(), "backend".to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            annotations: Some(
                [
                    ("team".to_string(), "platform".to_string()),
                    ("checked".to_string(), "yes".to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            owner_references: Some(vec![OwnerReference {
                api_version: "apps/v1".into(),
                kind: "ReplicaSet".into(),
                name: "api-7d9f".into(),
                uid: "owner-1".into(),
                controller: Some(true),
                ..Default::default()
            }]),
            managed_fields: Some(vec![
                ManagedFieldsEntry {
                    manager: Some("kubelet".into()),
                    operation: Some("Update".into()),
                    ..Default::default()
                },
                ManagedFieldsEntry {
                    manager: Some("kube-controller-manager".into()),
                    operation: Some("Update".into()),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        },
        spec: Some(PodSpec {
            node_name: Some("node-a".into()),
            service_account_name: Some("api".into()),
            termination_grace_period_seconds: Some(30),
            tolerations: Some(vec![Toleration {
                key: Some("dedicated".into()),
                operator: Some("Equal".into()),
                value: Some("api".into()),
                effect: Some("NoSchedule".into()),
                ..Default::default()
            }]),
            containers: vec![Container {
                name: "app".into(),
                ..Default::default()
            }],
            ..Default::default()
        }),
        status: Some(PodStatus {
            phase: Some("Running".into()),
            qos_class: Some("Burstable".into()),
            host_ips: Some(vec![HostIP {
                ip: "10.0.0.1".into(),
            }]),
            pod_ips: Some(vec![PodIP {
                ip: "10.1.0.7".into(),
            }]),
            conditions: Some(vec![
                PodCondition {
                    type_: "Ready".into(),
                    status: "True".into(),
                    ..Default::default()
                },
                PodCondition {
                    type_: "PodScheduled".into(),
                    status: "False".into(),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        }),
    }
}

pub(super) fn at(second: i64) -> Timestamp {
    Timestamp::from_second(second).unwrap()
}
