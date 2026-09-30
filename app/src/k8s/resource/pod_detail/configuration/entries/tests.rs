//! What the Configuration tab lists (`pod-configuration-tab` 2.1).

use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::pod_detail::configuration::entries::{ConfigEntry, entries};
use k8s_openapi::api::core::v1::Pod;
use serde_json::json;

fn pod(spec: serde_json::Value) -> Pod {
    serde_json::from_value(json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": { "name": "web-1", "namespace": "staging" },
        "spec": spec,
    }))
    .expect("a valid pod")
}

fn secret(name: &str) -> ObjectRef {
    ObjectRef::core("Secret", "staging", name)
}

fn config_map(name: &str) -> ObjectRef {
    ObjectRef::core("ConfigMap", "staging", name)
}

/// A Secret mounted as a volume and read by `valueFrom` is one entry, with
/// both uses - and the mount's path and container are named.
#[test]
fn one_entry_per_object_listing_every_use() {
    let pod = pod(json!({
        "containers": [{
            "name": "web",
            "volumeMounts": [{ "name": "creds", "mountPath": "/etc/creds" }],
            "env": [{ "name": "DB_PASSWORD", "valueFrom": { "secretKeyRef": { "name": "db", "key": "password" } } }],
        }],
        "volumes": [{ "name": "creds", "secret": { "secretName": "db" } }],
    }));

    assert_eq!(
        entries(&pod),
        vec![ConfigEntry {
            target: secret("db"),
            uses: vec![
                "volume `creds` mounted at `/etc/creds` in `web`".into(),
                "`DB_PASSWORD` from key `password` in `web`".into(),
            ],
        }]
    );
}

/// Every source kind is listed, in first-seen order: volumes (plain and
/// projected), then containers' `envFrom`, then image pull secrets.
#[test]
fn every_source_in_first_seen_order() {
    let pod = pod(json!({
        "containers": [{
            "name": "web",
            "envFrom": [{ "configMapRef": { "name": "app-env" } }],
        }],
        "volumes": [
            { "name": "config", "configMap": { "name": "app-config" } },
            { "name": "bundle", "projected": { "sources": [
                { "configMap": { "name": "ca-bundle" } },
                { "secret": { "name": "api-token" } },
            ] } },
        ],
        "imagePullSecrets": [{ "name": "registry" }],
    }));

    let listed: Vec<(ObjectRef, Vec<String>)> = entries(&pod)
        .into_iter()
        .map(|entry| (entry.target, entry.uses))
        .collect();
    assert_eq!(
        listed,
        vec![
            (
                config_map("app-config"),
                vec!["volume `config`".to_string()]
            ),
            (config_map("ca-bundle"), vec!["volume `bundle`".to_string()]),
            (secret("api-token"), vec!["volume `bundle`".to_string()]),
            (
                config_map("app-env"),
                vec!["`envFrom` in `web`".to_string()]
            ),
            (secret("registry"), vec!["image pull secret".to_string()]),
        ]
    );
}

/// A pod that references nothing lists nothing.
#[test]
fn a_pod_with_no_references_lists_nothing() {
    let pod = pod(json!({ "containers": [{ "name": "web" }] }));
    assert!(entries(&pod).is_empty());
}
