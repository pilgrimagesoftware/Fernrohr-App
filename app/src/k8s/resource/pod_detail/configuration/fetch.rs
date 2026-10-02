//! Reading one card's object: a ConfigMap's data, or a Secret reduced to its
//! type and key sizes before anything is kept.

use super::state::CardContents;
use crate::k8s::object_ref::ObjectRef;
use k8s_openapi::api::core::v1::{ConfigMap, Secret};
use kube::Api;

pub(in crate::k8s::resource::pod_detail) async fn fetch_card(
    client: kube::Client,
    target: ObjectRef,
) -> CardContents {
    let namespace = target.namespace.clone().unwrap_or_default();
    let failed = |error: kube::Error| match error {
        kube::Error::Api(status) if status.code == 404 => CardContents::NotFound,
        error => CardContents::Failed(crate::k8s::error::describe(&error)),
    };
    match target.kind.as_str() {
        "ConfigMap" => match Api::<ConfigMap>::namespaced(client, &namespace)
            .get(&target.name)
            .await
        {
            Ok(config_map) => CardContents::ConfigMap {
                data: config_map.data.unwrap_or_default().into_iter().collect(),
                binary: config_map
                    .binary_data
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(key, bytes)| (key, bytes.0.len()))
                    .collect(),
            },
            Err(error) => failed(error),
        },
        "Secret" => match Api::<Secret>::namespaced(client, &namespace)
            .get(&target.name)
            .await
        {
            // Sizes are read and the object dropped here: no Secret value
            // outlives this arm.
            Ok(secret) => CardContents::Secret {
                type_: secret.type_,
                keys: secret
                    .data
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(key, bytes)| (key, bytes.0.len()))
                    .collect(),
            },
            Err(error) => failed(error),
        },
        other => CardContents::Failed(format!("not a ConfigMap or Secret: {other}")),
    }
}
