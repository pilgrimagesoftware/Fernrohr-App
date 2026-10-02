//! The all-namespaces pod watch.

use super::*;

/// Starts a `kube_runtime::watcher` for all Pods across every namespace on
/// `client` and applies its events to `table` as they arrive. The stream,
/// reconnect and 401 handling are [`crate::k8s::cluster::watch_stream::run`]'s,
/// shared with every other kind's watch; a 401 stops the watch and calls
/// `on_unauthorized` once.
pub fn watch_all_namespaces(
    client: kube::Client,
    table: gpui_kit::Entity<PodsTable>,
    on_unauthorized: impl FnOnce(&mut gpui_kit::App) + 'static,
    cx: &mut gpui_kit::App,
) -> gpui_kit::Task<()> {
    let api: kube::Api<Pod> = kube::Api::all(client);
    crate::k8s::cluster::watch_stream::run(
        api,
        move |event, cx| {
            table.update(cx, |table, cx| {
                table.apply(event);
                cx.notify();
            });
        },
        on_unauthorized,
        cx,
    )
}
