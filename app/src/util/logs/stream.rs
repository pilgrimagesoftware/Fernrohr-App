//! Streaming a pod container's logs onto a [`LogsView`]: spawning the
//! producer on the tokio runtime, draining its [`LogEvent`]s line by line
//! onto the view, and the `kube`-specific stream that reads one container's
//! logs off the API.

use super::*;
use std::future::Future;

/// Spawns `produce` on the tokio runtime and applies its [`LogEvent`]s to
/// `view` as they arrive, on `view`'s own line-by-line channel - never
/// through the coalescing drain, so every line lands.
///
/// Returns the foreground [`gpui_kit::Task`] driving the drain; drop it (or
/// let a caller-held handle drop) to stop applying further lines, e.g. when
/// switching containers or closing the panel. Production callers that want
/// fire-and-forget behavior can `.detach()` the result themselves.
pub fn start_stream<F, Fut>(
    view: gpui_kit::Entity<LogsView>,
    cx: &mut gpui_kit::App,
    capacity: usize,
    produce: F,
) -> gpui_kit::Task<()>
where
    F: FnOnce(tokio::sync::mpsc::Sender<LogEvent>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let rx = crate::runtime::spawn_stream(cx, capacity, produce);
    cx.spawn(async move |cx| {
        crate::runtime::drain(rx, |event| {
            view.update(cx, |view, cx| {
                view.apply(event);
                cx.notify();
            });
        })
        .await;
    })
}

/// The readable half of a failure to *start* a pod's log stream: a plain
/// sentence for the API's 404 - which `kube::Error`'s own `Display` renders as
/// the unreadable `ApiError: pods "..." not found (...)` at the root of
/// `1-window-context-bar` bug 2 - and [`crate::k8s::error::describe`]'s general
/// rendering for every other [`kube::Error`]. A stream that *breaks* after it
/// started (below, the `std::io::Error` branch) has no pod/namespace/context to
/// name this precisely for, so that path keeps the error's own `Display`.
fn describe_log_stream_error(
    error: &kube::Error,
    pod_name: &str,
    namespace: &str,
    context_name: &str,
) -> String {
    match error {
        kube::Error::Api(status) if status.code == 404 => {
            format!("Pod {pod_name} not found in namespace {namespace} on {context_name}.")
        }
        other => crate::k8s::error::describe(other),
    }
}

/// Streams `container`'s logs in `namespace`/`pod_name` on `client`, line by
/// line, until the stream ends or the returned `Task` is dropped. A failure
/// to start the stream (e.g. the container hasn't started yet) reports
/// through the same channel as [`LogEvent::RequestFailed`] rather than
/// erroring the caller, matching [`LogsView`]'s own terminal-state handling.
/// `context_name` names the failure only - `client` already carries the
/// context to stream from.
pub fn stream_container_logs(
    client: kube::Client,
    namespace: String,
    pod_name: String,
    container: String,
    context_name: String,
    view: gpui_kit::Entity<LogsView>,
    cx: &mut gpui_kit::App,
) -> gpui_kit::Task<()> {
    start_stream(view, cx, 64, move |tx| async move {
        use futures_util::{AsyncBufReadExt, StreamExt};
        use k8s_openapi::api::core::v1::Pod;
        use kube::Api;
        use kube::api::LogParams;

        let api: Api<Pod> = Api::namespaced(client, &namespace);
        let lp = LogParams {
            container: Some(container),
            follow: true,
            ..Default::default()
        };
        let stream = match api.log_stream(&pod_name, &lp).await {
            Ok(stream) => stream,
            Err(error) => {
                let message =
                    describe_log_stream_error(&error, &pod_name, &namespace, &context_name);
                let detail = crate::k8s::error::detail(&error);
                let _ = tx.send(LogEvent::RequestFailed { message, detail }).await;
                return;
            }
        };
        let mut lines = stream.lines();
        loop {
            match lines.next().await {
                Some(Ok(line)) => {
                    if tx.send(LogEvent::Line(line)).await.is_err() {
                        break;
                    }
                }
                None => {
                    let _ = tx.send(LogEvent::Ended).await;
                    break;
                }
                Some(Err(error)) => {
                    let message = error.to_string();
                    let detail = format!("{error:?}");
                    let _ = tx.send(LogEvent::RequestFailed { message, detail }).await;
                    break;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests;
