//! A container's expanded detail: env, volume mounts, probes, command/args
//! and security context.

use super::fixtures::{field, rich_pod};
use crate::k8s::resource::pod_detail::fields::pod_fields;
use crate::k8s::resource::pod_detail::model::{ContainerSummary, EnvValue, PodFieldValue};
use jiff::Timestamp;
use k8s_openapi::api::core::v1::{
    Capabilities, Container, EnvVar, EnvVarSource, HTTPGetAction, Pod, Probe, SecretKeySelector,
    SecurityContext, VolumeMount,
};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;

/// A value that must never reach the detail: set as a stray literal beside a
/// Secret ref, it is what a careless projection would show.
const SECRET_VALUE: &str = "hunter2-do-not-show";

fn only_container(pod: &Pod) -> ContainerSummary {
    let fields = pod_fields(pod, Timestamp::from_second(0).unwrap());
    let PodFieldValue::Containers(containers) = &field(&fields, "Containers").unwrap().value else {
        panic!("containers render as PodFieldValue::Containers");
    };
    assert_eq!(containers.len(), 1);
    containers[0].clone()
}

/// Section 1.1: a literal env var, a Secret-sourced one, a volume mount and
/// a readiness probe each render, the Secret-sourced one as its reference.
#[test]
fn expanded_detail_renders_env_mounts_probes_and_secret_refs() {
    let mut pod = rich_pod();
    pod.spec.as_mut().unwrap().containers = vec![Container {
        name: "app".into(),
        command: Some(vec!["/bin/server".into()]),
        args: Some(vec!["--port".into(), "8080".into()]),
        env: Some(vec![
            EnvVar {
                name: "LOG_LEVEL".into(),
                value: Some("debug".into()),
                ..Default::default()
            },
            EnvVar {
                name: "DB_PASSWORD".into(),
                value_from: Some(EnvVarSource {
                    secret_key_ref: Some(SecretKeySelector {
                        name: "db-creds".into(),
                        key: "password".into(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
        ]),
        volume_mounts: Some(vec![VolumeMount {
            name: "data".into(),
            mount_path: "/var/lib/data".into(),
            read_only: Some(true),
            ..Default::default()
        }]),
        readiness_probe: Some(Probe {
            http_get: Some(HTTPGetAction {
                path: Some("/healthz".into()),
                port: IntOrString::Int(8080),
                ..Default::default()
            }),
            period_seconds: Some(10),
            ..Default::default()
        }),
        security_context: Some(SecurityContext {
            run_as_non_root: Some(true),
            capabilities: Some(Capabilities {
                drop: Some(vec!["ALL".into()]),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }];

    let detail = only_container(&pod).detail;

    assert_eq!(detail.env.len(), 2);
    assert_eq!(detail.env[0].name, "LOG_LEVEL");
    assert_eq!(detail.env[0].value, EnvValue::Literal("debug".into()));
    assert_eq!(detail.env[1].name, "DB_PASSWORD");
    assert_eq!(
        detail.env[1].value,
        EnvValue::Reference("from Secret db-creds key password".into())
    );
    assert_eq!(detail.volume_mounts, vec!["data -> /var/lib/data (ro)"]);
    assert_eq!(
        detail.probes,
        vec!["Readiness: HTTP GET /healthz:8080 every 10s"]
    );
    assert_eq!(detail.command, vec!["/bin/server"]);
    assert_eq!(detail.args, vec!["--port", "8080"]);
    assert_eq!(
        detail.security_context,
        vec!["runAsNonRoot=true", "capabilities.drop=ALL"]
    );
}

/// A `valueFrom` entry wins over a stray literal `value`, since that is the
/// source the kubelet uses - so a literal set next to a Secret ref is never
/// shown as though it were the value.
#[test]
fn a_secret_ref_hides_a_literal_set_alongside_it() {
    let mut pod = rich_pod();
    pod.spec.as_mut().unwrap().containers = vec![Container {
        name: "app".into(),
        env: Some(vec![EnvVar {
            name: "TOKEN".into(),
            value: Some(SECRET_VALUE.into()),
            value_from: Some(EnvVarSource {
                secret_key_ref: Some(SecretKeySelector {
                    name: "api-token".into(),
                    key: "token".into(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        }]),
        ..Default::default()
    }];

    let detail = only_container(&pod).detail;

    assert_eq!(
        detail.env[0].text(),
        "TOKEN (from Secret api-token key token)"
    );
    assert!(!format!("{detail:?}").contains(SECRET_VALUE));
}

/// A container that declares none of the expanded fields gets an empty
/// detail rather than placeholder rows.
#[test]
fn a_bare_container_has_empty_detail() {
    let mut pod = rich_pod();
    pod.spec.as_mut().unwrap().containers = vec![Container {
        name: "app".into(),
        ..Default::default()
    }];

    assert_eq!(only_container(&pod).detail, Default::default());
}
