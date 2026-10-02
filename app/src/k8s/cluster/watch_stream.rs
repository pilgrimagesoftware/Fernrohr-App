//! One `kube_runtime` watch for any resource type, consumed onto the main
//! thread, and how its failures are told apart. The Pods watch and the generic
//! list watch both run through [`run`], so reconnect, backoff and the 401 path
//! live in one place (`standard-resource-panels` design D3).

use futures_util::StreamExt as _;
use gpui_kit::{App, Task};
use kube::Api;
use kube_runtime::WatchStreamExt as _;
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

/// The API server's message when it refused the watch outright with a 403 - the user
/// may not list or watch this kind. `None` for any other error. A refusal won't change
/// by retrying, so a caller that can show it (`standard-resource-panels` D3: "A kind the
/// user cannot list") stops there instead.
pub(crate) fn refusal(error: &watcher::Error) -> Option<String> {
    match error {
        watcher::Error::InitialListFailed(kube::Error::Api(status))
        | watcher::Error::WatchStartFailed(kube::Error::Api(status))
            if status.code == 403 =>
        {
            Some(if status.message.is_empty() {
                "Forbidden".to_string()
            } else {
                status.message.clone()
            })
        }
        _ => None,
    }
}

/// Called once with the API server's message when it refuses a watch.
pub(crate) type ReportRefusal = Box<dyn FnOnce(String, &mut App)>;

/// What to do with a refused (403) watch: keep retrying it - the Pods watch's long-
/// standing behaviour - or stop and report the server's message once.
pub(crate) enum OnRefused {
    Retry,
    Report(ReportRefusal),
}

/// What to do when the initial list carries no `resourceVersion`, so no watch
/// can start from it (`watcher::Error::NoResourceVersion` - `componentstatuses`
/// does this): keep retrying it, or stop and say so once, so the caller can
/// fall back to polling (`unwatchable-kinds`).
pub(crate) enum OnNoResourceVersion {
    Retry,
    Report(Box<dyn FnOnce(&mut App)>),
}

/// One outcome off the watch stream: a normal event to apply, a 401 - which ends this
/// watch (the caller decides whether/how to restart it with a refreshed credential) - or
/// a reported 403, which also ends it.
enum Outcome<K> {
    Event(Box<watcher::Event<K>>),
    Unauthorized,
    Refused(String),
    NoResourceVersion,
}

/// Starts a `kube_runtime::watcher` over `api` and hands each event to `on_event`
/// on the main thread as it arrives. After a transient stream error the watcher
/// relists, with `kube_runtime`'s default backoff between attempts - its raw stream
/// retries immediately otherwise, which a persistent error turns into a tight loop
/// against the API server. Two errors end the watch instead: a 401, which it can't
/// recover from itself (the same expired credential would just come back), so it calls
/// `on_unauthorized` once; and, when `on_refused` or `on_no_resource_version` asks
/// to report it, a 403 or a list with no `resourceVersion`.
///
/// Dropping the returned task ends the watch.
pub(crate) fn run<K>(
    api: Api<K>,
    on_event: impl FnMut(watcher::Event<K>, &mut App) + 'static,
    on_unauthorized: impl FnOnce(&mut App) + 'static,
    on_refused: OnRefused,
    on_no_resource_version: OnNoResourceVersion,
    cx: &mut App,
) -> Task<()>
where
    K: kube::Resource + Clone + DeserializeOwned + Debug + Send + 'static,
{
    run_with(
        api,
        watcher::Config::default(),
        on_event,
        on_unauthorized,
        on_refused,
        on_no_resource_version,
        cx,
    )
}

/// [`run`], watching only what `config` selects - a field selector pinning one
/// object's events, say.
pub(crate) fn run_with<K>(
    api: Api<K>,
    config: watcher::Config,
    mut on_event: impl FnMut(watcher::Event<K>, &mut App) + 'static,
    on_unauthorized: impl FnOnce(&mut App) + 'static,
    on_refused: OnRefused,
    on_no_resource_version: OnNoResourceVersion,
    cx: &mut App,
) -> Task<()>
where
    K: kube::Resource + Clone + DeserializeOwned + Debug + Send + 'static,
{
    let report_refusal = matches!(on_refused, OnRefused::Report(_));
    let report_no_version = matches!(on_no_resource_version, OnNoResourceVersion::Report(_));
    let rx = crate::runtime::spawn_stream(cx, 64, move |tx| async move {
        let mut stream = Box::pin(watcher::watcher(api, config).default_backoff());
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
                Err(watcher::Error::NoResourceVersion) if report_no_version => {
                    let _ = tx.send(Outcome::NoResourceVersion).await;
                    return;
                }
                Err(error) => {
                    if report_refusal && let Some(message) = refusal(&error) {
                        let _ = tx.send(Outcome::Refused(message)).await;
                        return;
                    }
                }
            }
        }
    });
    cx.spawn(async move |cx| {
        let mut on_unauthorized = Some(on_unauthorized);
        let mut on_refused = match on_refused {
            OnRefused::Report(report) => Some(report),
            OnRefused::Retry => None,
        };
        let mut on_no_resource_version = match on_no_resource_version {
            OnNoResourceVersion::Report(report) => Some(report),
            OnNoResourceVersion::Retry => None,
        };
        crate::runtime::drain(rx, move |outcome| match outcome {
            Outcome::Event(event) => cx.update(|cx| on_event(*event, cx)),
            Outcome::Unauthorized => {
                if let Some(on_unauthorized) = on_unauthorized.take() {
                    cx.update(|cx| on_unauthorized(cx));
                }
            }
            Outcome::Refused(message) => {
                if let Some(report) = on_refused.take() {
                    cx.update(|cx| report(message, cx));
                }
            }
            Outcome::NoResourceVersion => {
                if let Some(report) = on_no_resource_version.take() {
                    cx.update(|cx| report(cx));
                }
            }
        })
        .await;
    })
}

#[cfg(test)]
mod tests;
