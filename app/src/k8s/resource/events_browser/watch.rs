//! The typed all-namespaces Event watch behind every events browser on a context
//! (design D1). Its reconnect, backoff and 401 handling are
//! `watch_stream::run`'s, shared with every other watch.

use super::store::EventsTable;
use crate::k8s::cluster::watch_stream::{self, OnNoResourceVersion, OnRefused};
use gpui_kit::{App, Entity, Task};
use k8s_openapi::api::core::v1::Event as K8sEvent;
use kube::{Api, Client};

/// Watches every core/v1 `Event` the client can see, across all namespaces -
/// the panel narrows to its namespace scope - applying each change to `table`.
/// Both event APIs are read through core/v1, where the API server also exposes
/// `events.k8s.io/v1`'s fields. A 401 stops it and calls `on_unauthorized`
/// once; a 403 stops it and records the refusal on `table`.
pub fn watch_events(
    client: Client,
    table: Entity<EventsTable>,
    on_unauthorized: impl FnOnce(&mut App) + 'static,
    cx: &mut App,
) -> Task<()> {
    let api: Api<K8sEvent> = Api::all(client);
    let refused = table.clone();
    watch_stream::run(
        api,
        move |event, cx| {
            table.update(cx, |table, cx| {
                table.apply(event);
                cx.notify();
            });
        },
        on_unauthorized,
        OnRefused::Report(Box::new(move |message, cx| {
            refused.update(cx, |table, cx| {
                table.set_refused(message);
                cx.notify();
            });
        })),
        // An Event list always carries a `resourceVersion`.
        OnNoResourceVersion::Retry,
        cx,
    )
}
