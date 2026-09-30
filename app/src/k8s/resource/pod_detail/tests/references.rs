//! The objects a pod names, as typed references: owners, the Namespace, Node
//! and Service Account rows, volumes, container env sources, and image pull
//! secrets. `resource-links` sections 1.2-1.4.

use super::fixtures::{field, rich_pod};
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::pod_detail::fields::pod_fields;
use crate::k8s::resource::pod_detail::model::{PodField, PodFieldValue, VolumeRow};
use jiff::Timestamp;
use k8s_openapi::api::core::v1::{
    ConfigMapEnvSource, ConfigMapKeySelector, ConfigMapProjection, ConfigMapVolumeSource,
    EmptyDirVolumeSource, EnvFromSource, EnvVar, EnvVarSource, LocalObjectReference,
    PersistentVolumeClaimVolumeSource, ProjectedVolumeSource, SecretKeySelector, SecretProjection,
    SecretVolumeSource, Volume, VolumeProjection,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;

fn fields_of(pod: &k8s_openapi::api::core::v1::Pod) -> Vec<PodField> {
    pod_fields(pod, Timestamp::from_second(0).unwrap())
}

fn targets(fields: &[PodField], label: &str) -> Vec<ObjectRef> {
    match &field(fields, label).unwrap().value {
        PodFieldValue::References { targets, .. } => targets.clone(),
        other => panic!("{label} should be references, was {other:?}"),
    }
}

/// 1.2: Namespace, Node and Service Account are typed references, filled in
/// with the right scope - Node and Namespace cluster-scoped, the service
/// account in the pod's namespace.
#[test]
fn single_references_carry_their_kind_and_scope() {
    let fields = fields_of(&rich_pod());

    assert_eq!(
        targets(&fields, "Namespace"),
        vec![ObjectRef::cluster_scoped("", "Namespace", "staging")]
    );
    assert_eq!(
        targets(&fields, "Node"),
        vec![ObjectRef::cluster_scoped("", "Node", "node-a")]
    );
    assert_eq!(
        targets(&fields, "Service Account"),
        vec![ObjectRef::core("ServiceAccount", "staging", "api")]
    );
}

/// 1.2: two owners are two references, not one joined line.
#[test]
fn each_owner_is_its_own_reference() {
    let mut pod = rich_pod();
    pod.metadata
        .owner_references
        .as_mut()
        .unwrap()
        .push(OwnerReference {
            api_version: "batch/v1".into(),
            kind: "Job".into(),
            name: "migrate".into(),
            uid: "owner-2".into(),
            ..Default::default()
        });

    let owners = targets(&fields_of(&pod), "Controlled By");

    assert_eq!(
        owners,
        vec![
            ObjectRef::namespaced("apps", "ReplicaSet", "staging", "api-7d9f"),
            ObjectRef::namespaced("batch", "Job", "staging", "migrate"),
        ]
    );
}

/// 1.3: a volume backed by an object carries that object as a reference, a
/// projected volume one per source, and an `emptyDir` none.
#[test]
fn volumes_are_named_typed_and_reference_their_sources() {
    let mut pod = rich_pod();
    pod.spec.as_mut().unwrap().volumes = Some(vec![
        Volume {
            name: "config".into(),
            config_map: Some(ConfigMapVolumeSource {
                name: "app-config".into(),
                ..Default::default()
            }),
            ..Default::default()
        },
        Volume {
            name: "tls".into(),
            secret: Some(SecretVolumeSource {
                secret_name: Some("app-tls".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        Volume {
            name: "data".into(),
            persistent_volume_claim: Some(PersistentVolumeClaimVolumeSource {
                claim_name: "app-data".into(),
                ..Default::default()
            }),
            ..Default::default()
        },
        Volume {
            name: "bundle".into(),
            projected: Some(ProjectedVolumeSource {
                sources: Some(vec![
                    VolumeProjection {
                        config_map: Some(ConfigMapProjection {
                            name: "ca-bundle".into(),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    VolumeProjection {
                        secret: Some(SecretProjection {
                            name: "api-token".into(),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                ]),
                ..Default::default()
            }),
            ..Default::default()
        },
        Volume {
            name: "scratch".into(),
            empty_dir: Some(EmptyDirVolumeSource::default()),
            ..Default::default()
        },
    ]);

    let fields = fields_of(&pod);
    let PodFieldValue::Volumes(volumes) = &field(&fields, "Volumes").unwrap().value else {
        panic!("volumes render as PodFieldValue::Volumes");
    };

    let rows: Vec<String> = volumes.iter().map(VolumeRow::text).collect();
    assert_eq!(
        rows,
        vec![
            "config: ConfigMap: app-config",
            "tls: Secret: app-tls",
            "data: PersistentVolumeClaim: app-data",
            "bundle: Projected: ca-bundle, api-token",
            "scratch: EmptyDir",
        ]
    );
    let references: Vec<&[ObjectRef]> = volumes
        .iter()
        .map(|volume| volume.references.as_slice())
        .collect();
    assert_eq!(
        references,
        vec![
            &[ObjectRef::core("ConfigMap", "staging", "app-config")][..],
            &[ObjectRef::core("Secret", "staging", "app-tls")][..],
            &[ObjectRef::core(
                "PersistentVolumeClaim",
                "staging",
                "app-data"
            )][..],
            &[
                ObjectRef::core("ConfigMap", "staging", "ca-bundle"),
                ObjectRef::core("Secret", "staging", "api-token"),
            ][..],
            &[][..],
        ]
    );
}

fn env_from_config_map(key: &str, config_map: &str) -> EnvVar {
    EnvVar {
        name: key.into(),
        value_from: Some(EnvVarSource {
            config_map_key_ref: Some(ConfigMapKeySelector {
                name: config_map.into(),
                key: key.into(),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn container_env_sources(pod: &k8s_openapi::api::core::v1::Pod) -> Vec<ObjectRef> {
    let fields = fields_of(pod);
    let PodFieldValue::Containers(containers) = &field(&fields, "Containers").unwrap().value else {
        panic!("containers render as cards");
    };
    containers[0].env_sources.clone()
}

/// 1.4: several keys read from one ConfigMap are one reference.
#[test]
fn keys_from_one_config_map_are_one_reference() {
    let mut pod = rich_pod();
    pod.spec.as_mut().unwrap().containers[0].env = Some(vec![
        env_from_config_map("LOG_LEVEL", "app-config"),
        env_from_config_map("PORT", "app-config"),
        env_from_config_map("REGION", "app-config"),
    ]);

    assert_eq!(
        container_env_sources(&pod),
        vec![ObjectRef::core("ConfigMap", "staging", "app-config")]
    );
}

/// 1.4: a ConfigMap and a Secret are two references, whole (`envFrom`) and
/// by-key (`valueFrom`) sources both count, and first-seen order holds.
#[test]
fn a_config_map_and_a_secret_are_two_references_in_first_seen_order() {
    let mut pod = rich_pod();
    let container = &mut pod.spec.as_mut().unwrap().containers[0];
    container.env_from = Some(vec![EnvFromSource {
        secret_ref: Some(k8s_openapi::api::core::v1::SecretEnvSource {
            name: "app-secrets".into(),
            ..Default::default()
        }),
        ..Default::default()
    }]);
    container.env = Some(vec![
        env_from_config_map("LOG_LEVEL", "app-config"),
        EnvVar {
            name: "TOKEN".into(),
            value_from: Some(EnvVarSource {
                secret_key_ref: Some(SecretKeySelector {
                    name: "app-secrets".into(),
                    key: "token".into(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        },
    ]);

    assert_eq!(
        container_env_sources(&pod),
        vec![
            ObjectRef::core("Secret", "staging", "app-secrets"),
            ObjectRef::core("ConfigMap", "staging", "app-config"),
        ]
    );
}

/// `envFrom` with a whole ConfigMap counts the same as reading its keys.
#[test]
fn a_whole_config_map_env_source_is_a_reference() {
    let mut pod = rich_pod();
    pod.spec.as_mut().unwrap().containers[0].env_from = Some(vec![EnvFromSource {
        config_map_ref: Some(ConfigMapEnvSource {
            name: "app-config".into(),
            ..Default::default()
        }),
        ..Default::default()
    }]);

    assert_eq!(
        container_env_sources(&pod),
        vec![ObjectRef::core("ConfigMap", "staging", "app-config")]
    );
}

/// 1.4: image pull secrets are a row of references in the pod's namespace.
#[test]
fn image_pull_secrets_are_references() {
    let mut pod = rich_pod();
    pod.spec.as_mut().unwrap().image_pull_secrets = Some(vec![
        LocalObjectReference {
            name: "registry-a".into(),
        },
        LocalObjectReference {
            name: "registry-b".into(),
        },
    ]);

    assert_eq!(
        targets(&fields_of(&pod), "Image Pull Secrets"),
        vec![
            ObjectRef::core("Secret", "staging", "registry-a"),
            ObjectRef::core("Secret", "staging", "registry-b"),
        ]
    );
}

/// A pod without image pull secrets has no such row, rather than an empty one.
#[test]
fn no_image_pull_secrets_means_no_row() {
    assert!(field(&fields_of(&rich_pod()), "Image Pull Secrets").is_none());
}
