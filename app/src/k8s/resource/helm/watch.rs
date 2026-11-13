//! The watch behind a context's [`HelmReleasesTable`] (helm-releases D1): Helm
//! storage Secrets, selected by `owner=helm` and `type=helm.sh/release.v1`,
//! as metadata only - their labels are all a row needs - and, for each
//! release's latest revision, one fetch of its payload, decoded off the UI
//! thread.

use super::decode::decode_release;
use super::table::{Fetch, HelmReleasesTable, Summary};
use crate::k8s::cluster::watch_stream::{self, OnNoResourceVersion, OnRefused};
use gpui_kit::{App, Entity, Task, WeakEntity};
use k8s_openapi::api::core::v1::Secret;
use kube::core::PartialObjectMeta;
use kube::{Api, Client};
use kube_runtime::watcher;

/// The label every Helm storage Secret carries.
const OWNER_SELECTOR: &str = "owner=helm";
/// The type Helm 3 and 4 give a release revision's Secret.
const TYPE_SELECTOR: &str = "type=helm.sh/release.v1";

/// Which Secrets a release watch covers: every namespace, or one - a user may
/// read Secrets in a namespace without being allowed to list them all.
// UNWIRED(#370): the Helm panels (helm-releases section 3) pick a scope;
// until then only tests construct one.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum HelmScope {
    All,
    Namespace(String),
}

/// Starts the release watch for `scope`, filling `table`. Dropping the task
/// ends it.
pub(crate) fn watch_helm_releases(
    client: Client,
    scope: &HelmScope,
    table: Entity<HelmReleasesTable>,
    on_unauthorized: impl FnOnce(&mut App) + 'static,
    cx: &mut App,
) -> Task<()> {
    let api: Api<PartialObjectMeta<Secret>> = match scope {
        HelmScope::All => Api::all(client.clone()),
        HelmScope::Namespace(namespace) => Api::namespaced(client.clone(), namespace),
    };
    let config = watcher::Config::default()
        .labels(OWNER_SELECTOR)
        .fields(TYPE_SELECTOR);
    let weak = table.downgrade();
    let refused = table.downgrade();
    watch_stream::run_with(
        api,
        config,
        move |event, cx| {
            let fetches = weak
                .update(cx, |table, cx| {
                    table.apply(event);
                    cx.notify();
                    table.take_fetches()
                })
                .unwrap_or_default();
            for fetch in fetches {
                fetch_latest(client.clone(), fetch, weak.clone(), cx);
            }
        },
        on_unauthorized,
        OnRefused::Report(Box::new(move |message, cx| {
            let _ = refused.update(cx, |table, cx| {
                table.set_refused(message);
                cx.notify();
            });
        })),
        // A Secret list always carries a `resourceVersion`.
        OnNoResourceVersion::Retry,
        cx,
    )
}

/// Fetches and decodes `fetch`'s revision off the UI thread, then records its
/// summary in `table` - a weak handle, so a watch that ended meanwhile drops it.
fn fetch_latest(client: Client, fetch: Fetch, table: WeakEntity<HelmReleasesTable>, cx: &mut App) {
    let (namespace, secret) = (fetch.key.namespace.clone(), fetch.secret.clone());
    let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
        let api: Api<Secret> = Api::namespaced(client, &namespace);
        let summary = match api.get(&secret).await {
            Ok(secret) => summarize(&secret),
            Err(error) => Summary::Unreadable(fetch_failure(&error)),
        };
        let _ = tx.send(summary).await;
    });
    cx.spawn(async move |cx| {
        crate::runtime::drain(rx, |summary| {
            let _ = table.update(cx, |table, cx| {
                table.finish(&fetch, summary);
                cx.notify();
            });
        })
        .await;
    })
    .detach();
}

/// What a revision Secret's payload says, or why it couldn't be read.
fn summarize(secret: &Secret) -> Summary {
    let Some(payload) = secret.data.as_ref().and_then(|data| data.get("release")) else {
        return Summary::Unreadable("the revision holds no release".to_string());
    };
    match decode_release(&payload.0) {
        Ok(release) => Summary::Ready {
            chart: release.chart,
            info: release.info,
        },
        Err(error) => Summary::Unreadable(error.to_string()),
    }
}

/// A failed fetch, in the API server's words when it gave some.
fn fetch_failure(error: &kube::Error) -> String {
    match error {
        kube::Error::Api(status) if !status.message.is_empty() => status.message.clone(),
        _ => "the revision couldn't be fetched".to_string(),
    }
}
