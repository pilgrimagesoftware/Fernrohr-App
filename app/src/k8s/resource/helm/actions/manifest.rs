//! How many objects a release's deployed manifest declares - what Uninstall's
//! confirmation says it will delete. The manifest is read off the UI thread
//! and only its count leaves; its text is never shown or kept.

use crate::k8s::resource::helm::decode_release;
use gpui_kit::{App, Task};
use k8s_openapi::api::core::v1::Secret;
use kube::{Api, Client};
use serde::Deserialize as _;

/// The objects `manifest` declares: its YAML documents that name a `kind`.
/// Blank documents, comment-only ones and ones that don't parse aren't
/// counted.
pub(super) fn count_objects(manifest: &str) -> usize {
    serde_yaml_ng::Deserializer::from_str(manifest)
        .filter_map(|document| serde_yaml_ng::Value::deserialize(document).ok())
        .filter(|value| {
            value
                .get("kind")
                .and_then(serde_yaml_ng::Value::as_str)
                .is_some_and(|kind| !kind.is_empty())
        })
        .count()
}

/// Fetches revision Secret `secret` in `namespace` and counts its manifest's
/// objects, or `None` when it can't be read.
pub(super) fn fetch_count(
    client: Client,
    namespace: String,
    secret: String,
    cx: &App,
) -> Task<Option<usize>> {
    let counted = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
        let api: Api<Secret> = Api::namespaced(client, &namespace);
        let count = match api.get(&secret).await {
            Ok(secret) => tokio::task::spawn_blocking(move || {
                let payload = secret.data.as_ref()?.get("release")?.0.clone();
                let release = decode_release(&payload).ok()?;
                Some(count_objects(release.manifest.expose()?))
            })
            .await
            .ok()
            .flatten(),
            Err(_) => None,
        };
        let _ = tx.send(count).await;
    });
    cx.background_executor().spawn(async move {
        let mut counted = counted;
        counted.recv().await.flatten()
    })
}
