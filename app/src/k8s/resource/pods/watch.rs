//! The pod watch: across every namespace, or one namespace's (#457).

use super::*;

/// Starts a `kube_runtime::watcher` for the Pods on `client` - across every namespace,
/// or only `namespace`'s when one is given - and applies its events to `table` as they
/// arrive. The stream,
/// reconnect and 401 handling are [`crate::k8s::cluster::watch_stream::run`]'s,
/// shared with every other kind's watch; a 401 stops the watch and calls
/// `on_unauthorized` once, and a 403 stops it and records the server's refusal on
/// `table` (#446), so a reader can say pods can't be listed.
pub fn watch_pods(
    client: kube::Client,
    namespace: Option<&str>,
    table: gpui_kit::Entity<PodsTable>,
    on_unauthorized: impl FnOnce(&mut gpui_kit::App) + 'static,
    cx: &mut gpui_kit::App,
) -> gpui_kit::Task<()> {
    let api: kube::Api<Pod> = match namespace {
        Some(namespace) => kube::Api::namespaced(client, namespace),
        None => kube::Api::all(client),
    };
    let refused = table.clone();
    crate::k8s::cluster::watch_stream::run(
        api,
        move |event, cx| {
            table.update(cx, |table, cx| {
                table.apply(event);
                cx.notify();
            });
        },
        on_unauthorized,
        crate::k8s::cluster::watch_stream::OnRefused::Report(Box::new(move |message, cx| {
            refused.update(cx, |table, cx| {
                table.set_refused(message);
                cx.notify();
            });
        })),
        // A Pod list always carries a `resourceVersion`.
        crate::k8s::cluster::watch_stream::OnNoResourceVersion::Retry,
        cx,
    )
}
