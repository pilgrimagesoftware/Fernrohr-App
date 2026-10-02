//! One `kube_runtime` watch for any resource type, consumed onto the main
//! thread, and how its failures are told apart. The Pods watch and the generic
//! list watch both run through [`run`], so reconnect, backoff and the 401 path
//! live in one place (`standard-resource-panels` design D3).

use futures_util::StreamExt as _;
use gpui_kit::{App, Task};
use kube::Api;
use kube_runtime::watcher;
use serde::de::DeserializeOwned;
use std::fmt::Debug;

/// True for a watch stream error that came back as an HTTP 401 - an expired or otherwise
/// rejected credential, as opposed to a transient network error `kube_runtime`'s own
/// backoff already retries transparently. Section 7.3: this is what routes a stream error
/// through the exec-plugin-refresh path instead of the ordinary swallow-and-retry one.
pub(crate) fn is_unauthorized(error: &watcher::Error) -> bool {
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
enum Outcome<K> {
    Event(Box<watcher::Event<K>>),
    Unauthorized,
}

/// Starts a `kube_runtime::watcher` over `api` and hands each event to `on_event`
/// on the main thread as it arrives. Reconnect and backoff after a transient stream
/// error are `kube_runtime`'s own job; this just keeps consuming the stream - except a
/// 401, which the watch can't recover from itself (the same expired credential would
/// just come back), so it stops and calls `on_unauthorized` once instead of retrying
/// forever against a token that will never work.
///
/// Dropping the returned task ends the watch.
pub(crate) fn run<K>(
    api: Api<K>,
    mut on_event: impl FnMut(watcher::Event<K>, &mut App) + 'static,
    on_unauthorized: impl FnOnce(&mut App) + 'static,
    cx: &mut App,
) -> Task<()>
where
    K: kube::Resource + Clone + DeserializeOwned + Debug + Send + 'static,
{
    let rx = crate::runtime::spawn_stream(cx, 64, move |tx| async move {
        let mut stream = Box::pin(watcher::watcher(api, watcher::Config::default()));
        while let Some(event) = stream.next().await {
            match event {
                Ok(event) => {
                    if tx.send(Outcome::Event(Box::new(event))).await.is_err() {
                        break;
                    }
                }
                Err(error) if is_unauthorized(&error) => {
                    let _ = tx.send(Outcome::Unauthorized).await;
                    return;
                }
                Err(_) => continue,
            }
        }
    });
    cx.spawn(async move |cx| {
        let mut on_unauthorized = Some(on_unauthorized);
        crate::runtime::drain(rx, move |outcome| match outcome {
            Outcome::Event(event) => cx.update(|cx| on_event(*event, cx)),
            Outcome::Unauthorized => {
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
