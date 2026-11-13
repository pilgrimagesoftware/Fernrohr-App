//! One revision of a release, fetched and decoded for the detail panel: its
//! chart and info, and its notes. The values and manifest it also holds are
//! dropped here; the Values and Manifest tabs fetch their own.

use crate::k8s::resource::helm::{ChartMeta, ReleaseInfo, decode_release};
use crate::k8s::resource::secret_value::SecretValue;
use gpui_kit::{App, WeakEntity};
use k8s_openapi::api::core::v1::Secret;
use kube::{Api, Client};

/// What one revision says, beyond its labels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RevisionDetail {
    pub(crate) chart: ChartMeta,
    pub(crate) info: ReleaseInfo,
}

/// Where one revision's fetch is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Fetched {
    Loading,
    Ready(RevisionDetail),
    Unreadable(String),
}

/// A fetched revision: its detail, and its notes.
pub(crate) type Decoded = Result<(RevisionDetail, SecretValue), String>;

/// Fetches Secret `secret` in `namespace` and decodes it off the UI thread,
/// then hands the result to `done` on `panel` - a weak handle, so a closed
/// panel drops it.
pub(crate) fn fetch_revision<T: 'static>(
    client: Client,
    namespace: String,
    secret: String,
    panel: WeakEntity<T>,
    done: impl FnOnce(&mut T, Decoded, &mut gpui_kit::Context<T>) + 'static,
    cx: &mut App,
) {
    let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
        let api: Api<Secret> = Api::namespaced(client, &namespace);
        let decoded = match api.get(&secret).await {
            Ok(secret) => tokio::task::spawn_blocking(move || decode(&secret))
                .await
                .unwrap_or_else(|_| Err("the revision couldn't be read".to_string())),
            Err(kube::Error::Api(status)) if !status.message.is_empty() => Err(status.message),
            Err(_) => Err("the revision couldn't be fetched".to_string()),
        };
        let _ = tx.send(decoded).await;
    });
    cx.spawn(async move |cx| {
        let mut done = Some(done);
        crate::runtime::drain(rx, |decoded| {
            if let Some(done) = done.take() {
                let _ = panel.update(cx, |panel, cx| done(panel, decoded, cx));
            }
        })
        .await;
    })
    .detach();
}

fn decode(secret: &Secret) -> Decoded {
    let payload = secret
        .data
        .as_ref()
        .and_then(|data| data.get("release"))
        .ok_or_else(|| "the revision holds no release".to_string())?;
    let release = decode_release(&payload.0).map_err(|error| error.to_string())?;
    Ok((
        RevisionDetail {
            chart: release.chart,
            info: release.info,
        },
        release.notes,
    ))
}
