//! One kind's watch over `Api<DynamicObject>`.

use super::store::ObjectsTable;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::watch_stream::{self, OnRefused};
use gpui_kit::{App, Entity, Task};
use kube::api::{ApiResource, DynamicObject};
use kube::{Api, Client};

/// Starts a watch of every `kind` object the client can see - across all namespaces for
/// a namespaced kind, which the panel then narrows to its namespace selection - and
/// applies its events to `table`. A 401 stops it and calls `on_unauthorized` once; a 403
/// stops it and records the server's refusal on `table` for the panel to show.
pub fn watch_kind(
    client: Client,
    kind: &DiscoveredKind,
    table: Entity<ObjectsTable>,
    on_unauthorized: impl FnOnce(&mut App) + 'static,
    cx: &mut App,
) -> Task<()> {
    let resource = ApiResource::from_gvk_with_plural(&kind.gvk, &kind.plural);
    let api: Api<DynamicObject> = Api::all_with(client, &resource);
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
        cx,
    )
}
