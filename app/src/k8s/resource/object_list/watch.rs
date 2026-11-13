//! One kind's live rows over `Api<DynamicObject>`: a watch, or - for a kind that
//! can be listed but not watched - polling (`unwatchable-kinds`). A custom
//! resource's list first fetches its CRD's printer columns (#352).

use super::columns::{self, PrinterColumns};
use super::poll::poll_kind;
use super::store::{ListMode, ObjectsTable};
use crate::consts::CRD_COLUMNS_TIMEOUT;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::watch_stream::{self, OnNoResourceVersion, OnRefused, ReportRefusal};
use gpui_kit::{App, Entity, Task};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::api::{ApiResource, DynamicObject};
use kube::{Api, Client};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::{Notify, mpsc};

/// Keeps `table` current with every `kind` object the client can see - across all
/// namespaces for a namespaced kind, which the panel then narrows to its namespace
/// selection. A 401 stops it and calls `on_unauthorized` once; a 403 stops it and
/// records the server's refusal on `table` for the panel to show.
///
/// How depends on the kind's verbs, as discovery reported them:
/// - no `list`: nothing to fetch, so `table` is marked [`ListMode::Unlistable`];
/// - `list` but no `watch`: polled from the start;
/// - both: watched, falling back to polling if the list carries no
///   `resourceVersion` to start the watch from - `kube_runtime`'s
///   `NoResourceVersion`, which isn't a refusal and would otherwise be retried
///   silently forever, leaving the table empty.
///
/// One task either way, so the registry entry that owns it owns the poller too.
///
/// A kind with no columns of its own waits first for its CRD's printer
/// columns ([`printer_columns`]), so every row is built with their cells.
pub fn watch_kind(
    client: Client,
    kind: &DiscoveredKind,
    table: Entity<ObjectsTable>,
    on_unauthorized: impl FnOnce(&mut App) + 'static,
    cx: &mut App,
) -> Task<()> {
    let Some(mut printer) = printer_columns(client.clone(), kind, cx) else {
        return list_kind(client, kind, table, on_unauthorized, cx);
    };
    let kind = kind.clone();
    cx.spawn(async move |cx| {
        if let Some(printer) = printer.recv().await {
            cx.update(|cx| table.update(cx, |table, _| table.set_printer_columns(printer)));
        }
        let list = cx.update(|cx| list_kind(client, &kind, table, on_unauthorized, cx));
        list.await;
    })
}

/// Fetches `kind`'s CRD (`<plural>.<group>`) on the tokio runtime and sends
/// its printer columns for the version listed, if it declares any. `None` -
/// nothing to fetch - for a kind with columns of its own or in the core
/// group. A CRD that can't be read (forbidden, an aggregated API with no CRD,
/// [`CRD_COLUMNS_TIMEOUT`] passing) sends nothing: the list keeps the generic
/// columns.
fn printer_columns(
    client: Client,
    kind: &DiscoveredKind,
    cx: &App,
) -> Option<mpsc::Receiver<PrinterColumns>> {
    let group = &kind.gvk.group;
    if group.is_empty() || columns::for_kind(group, &kind.gvk.kind).is_some() {
        return None;
    }
    let name = format!("{}.{group}", kind.plural);
    let version = kind.gvk.version.clone();
    Some(crate::runtime::spawn_stream(cx, 1, move |tx| async move {
        let api: Api<CustomResourceDefinition> = Api::all(client);
        let fetched = tokio::time::timeout(CRD_COLUMNS_TIMEOUT, api.get(&name)).await;
        if let Ok(Ok(crd)) = fetched
            && let Some(printer) = PrinterColumns::from_crd(&crd, &version)
        {
            let _ = tx.send(printer).await;
        }
    }))
}

/// Lists `kind` into `table` by its verbs, as [`watch_kind`] describes.
fn list_kind(
    client: Client,
    kind: &DiscoveredKind,
    table: Entity<ObjectsTable>,
    on_unauthorized: impl FnOnce(&mut App) + 'static,
    cx: &mut App,
) -> Task<()> {
    let resource = ApiResource::from_gvk_with_plural(&kind.gvk, &kind.plural);
    let api: Api<DynamicObject> = Api::all_with(client, &resource);
    if !kind.verbs.list {
        table.update(cx, |table, cx| {
            table.set_mode(ListMode::Unlistable);
            cx.notify();
        });
        return Task::ready(());
    }
    if !kind.verbs.watch {
        return start_polling(api, table, on_unauthorized, cx);
    }
    let unwatchable = Rc::new(Cell::new(false));
    // The watch and, if it turns out unwatchable, the poller both need the
    // 401 callback; whichever runs first takes it.
    let on_unauthorized: SharedCallback = Rc::new(Cell::new(Some(Box::new(on_unauthorized))));
    let watch = {
        let table = table.clone();
        let refused = table.clone();
        let unwatchable = unwatchable.clone();
        let on_unauthorized = on_unauthorized.clone();
        watch_stream::run(
            api.clone(),
            move |event, cx| {
                table.update(cx, |table, cx| {
                    table.apply(event);
                    cx.notify();
                });
            },
            move |cx| {
                if let Some(on_unauthorized) = on_unauthorized.take() {
                    on_unauthorized(cx);
                }
            },
            OnRefused::Report(report_refusal(refused)),
            OnNoResourceVersion::Report(Box::new(move |_cx| unwatchable.set(true))),
            cx,
        )
    };
    cx.spawn(async move |cx| {
        watch.await;
        if !unwatchable.get() {
            return;
        }
        let poller = cx.update(|cx| {
            start_polling(
                api,
                table,
                move |cx| {
                    if let Some(on_unauthorized) = on_unauthorized.take() {
                        on_unauthorized(cx);
                    }
                },
                cx,
            )
        });
        poller.await;
    })
}

/// A one-shot callback both the watch and its fallback poller may run; whichever
/// takes it first does.
type SharedCallback = Rc<Cell<Option<Box<dyn FnOnce(&mut App)>>>>;

/// Marks `table` polled and starts polling `api` into it.
fn start_polling(
    api: Api<DynamicObject>,
    table: Entity<ObjectsTable>,
    on_unauthorized: impl FnOnce(&mut App) + 'static,
    cx: &mut App,
) -> Task<()> {
    let refresh = Arc::new(Notify::new());
    table.update(cx, |table, cx| {
        table.set_mode(ListMode::Polled {
            refresh: refresh.clone(),
        });
        cx.notify();
    });
    let refused = report_refusal(table.clone());
    poll_kind(api, table, refresh, on_unauthorized, refused, cx)
}

/// Records the server's refusal on `table`, for the panel to show.
fn report_refusal(table: Entity<ObjectsTable>) -> ReportRefusal {
    Box::new(move |message, cx| {
        table.update(cx, |table, cx| {
            table.set_refused(message);
            cx.notify();
        });
    })
}
