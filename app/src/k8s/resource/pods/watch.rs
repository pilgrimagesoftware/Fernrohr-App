//! The all-namespaces pod watch, and how its failures are told apart.

use super::*;

/// True for a watch stream error that came back as an HTTP 401 - an expired or otherwise
/// rejected credential, as opposed to a transient network error `kube_runtime`'s own
/// backoff already retries transparently. Section 7.3: this is what routes a stream error
/// through the exec-plugin-refresh path instead of the ordinary swallow-and-retry one.
pub(super) fn is_unauthorized(error: &watcher::Error) -> bool {
    let kube_error = match error {
        watcher::Error::InitialListFailed(error)
        | watcher::Error::WatchStartFailed(error)
        | watcher::Error::WatchFailed(error) => Some(error),
        watcher::Error::WatchError(_) | watcher::Error::NoResourceVersion => None,
    };
    matches!(kube_error, Some(kube::Error::Api(status)) if status.code == 401)
}

/// One outcome off the watch stream: a normal event to apply, or a 401 - which ends this
/// watch (the caller decides whether/how to restart it with a refreshed credential).
pub(super) enum WatchOutcome {
    Event(Box<watcher::Event<Pod>>),
    Unauthorized,
}

/// Starts a `kube_runtime::watcher` for all Pods across every namespace on
/// `client` and applies its events to `table` as they arrive. Reconnect and
/// backoff after a transient stream error are `kube_runtime`'s own job (design D3); this
/// just keeps consuming the stream - except a 401, which this watch has no way to recover
/// from itself (the same expired credential would just come back), so it stops and calls
/// `on_unauthorized` once instead of retrying forever against a token that will never work.
pub fn watch_all_namespaces(
    client: kube::Client,
    table: gpui_kit::Entity<PodsTable>,
    on_unauthorized: impl FnOnce(&mut gpui_kit::App) + Send + 'static,
    cx: &mut gpui_kit::App,
) -> gpui_kit::Task<()> {
    use futures_util::StreamExt;
    use kube::Api;

    let rx = crate::runtime::spawn_stream(cx, 64, move |tx| async move {
        let api: Api<Pod> = Api::all(client);
        let mut stream = Box::pin(watcher::watcher(api, watcher::Config::default()));
        while let Some(event) = stream.next().await {
            match event {
                Ok(event) => {
                    if tx.send(WatchOutcome::Event(Box::new(event))).await.is_err() {
                        break;
                    }
                }
                Err(error) if is_unauthorized(&error) => {
                    let _ = tx.send(WatchOutcome::Unauthorized).await;
                    return;
                }
                Err(_) => continue,
            }
        }
    });
    cx.spawn(async move |cx| {
        let mut on_unauthorized = Some(on_unauthorized);
        crate::runtime::drain(rx, move |outcome| match outcome {
            WatchOutcome::Event(event) => {
                table.update(cx, |table, cx| {
                    table.apply(*event);
                    cx.notify();
                });
            }
            WatchOutcome::Unauthorized => {
                if let Some(on_unauthorized) = on_unauthorized.take() {
                    cx.update(|cx| on_unauthorized(cx));
                }
            }
        })
        .await;
    })
}

#[cfg(test)]
mod tests;
