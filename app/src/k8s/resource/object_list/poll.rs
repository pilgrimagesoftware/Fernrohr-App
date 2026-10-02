//! Polling one kind that can be listed but not watched (`unwatchable-kinds`):
//! a list every `consts::LIST_POLL_INTERVAL`, or sooner when the panel asks
//! for a refresh, applied to the same [`ObjectsTable`] a watch would feed.

use super::store::ObjectsTable;
use crate::consts::LIST_POLL_INTERVAL;
use crate::k8s::cluster::watch_stream::ReportRefusal;
use gpui_kit::{App, Entity, Task};
use kube::Api;
use kube::api::{DynamicObject, ListParams};
use std::sync::Arc;
use tokio::sync::Notify;

/// One list's outcome, carried to the main thread.
enum Polled {
    Listed(Vec<DynamicObject>),
    Unauthorized,
    Refused(String),
}

/// Lists `api` now and then every [`LIST_POLL_INTERVAL`], or as soon as
/// `refresh` is notified, replacing `table`'s rows with each list. A 401 stops
/// polling and calls `on_unauthorized` once; a 403 stops it and reports the
/// server's message through `on_refused`. Any other failure keeps the last
/// rows and tries again on the next tick.
///
/// Dropping the returned task stops polling: the tokio side sees its channel
/// close and returns, even mid-wait.
pub fn poll_kind(
    api: Api<DynamicObject>,
    table: Entity<ObjectsTable>,
    refresh: Arc<Notify>,
    on_unauthorized: impl FnOnce(&mut App) + 'static,
    on_refused: ReportRefusal,
    cx: &mut App,
) -> Task<()> {
    let rx = crate::runtime::spawn_stream(cx, 4, move |tx| async move {
        loop {
            let polled = match api.list(&ListParams::default()).await {
                Ok(list) => Some(Polled::Listed(list.items)),
                Err(kube::Error::Api(status)) if status.code == 401 => Some(Polled::Unauthorized),
                Err(kube::Error::Api(status)) if status.code == 403 => {
                    Some(Polled::Refused(if status.message.is_empty() {
                        "Forbidden".to_string()
                    } else {
                        status.message
                    }))
                }
                Err(_) => None,
            };
            let stop = matches!(polled, Some(Polled::Unauthorized | Polled::Refused(_)));
            if let Some(polled) = polled
                && tx.send(polled).await.is_err()
            {
                return;
            }
            if stop {
                return;
            }
            tokio::select! {
                _ = tokio::time::sleep(LIST_POLL_INTERVAL) => {}
                _ = refresh.notified() => {}
                _ = tx.closed() => return,
            }
        }
    });
    cx.spawn(async move |cx| {
        let mut on_unauthorized = Some(on_unauthorized);
        let mut on_refused = Some(on_refused);
        crate::runtime::drain(rx, move |polled| match polled {
            Polled::Listed(objects) => cx.update(|cx| {
                table.update(cx, |table, cx| {
                    table.replace_all(objects);
                    cx.notify();
                })
            }),
            Polled::Unauthorized => {
                if let Some(on_unauthorized) = on_unauthorized.take() {
                    cx.update(|cx| on_unauthorized(cx));
                }
            }
            Polled::Refused(message) => {
                if let Some(report) = on_refused.take() {
                    cx.update(|cx| report(message, cx));
                }
            }
        })
        .await;
    })
}
