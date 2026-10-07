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
    start_stream_with(view, cx, capacity, LogsView::apply, produce)
}

/// [`start_stream`] for one of several containers sharing `view` (#150): each
/// event lands as a line tagged with `source` ([`LogsView::apply_from`]), so
/// one container's stream ending doesn't end the panel's.
pub fn start_tagged_stream<F, Fut>(
    view: gpui_kit::Entity<LogsView>,
    cx: &mut gpui_kit::App,
    capacity: usize,
    source: String,
    produce: F,
) -> gpui_kit::Task<()>
where
    F: FnOnce(tokio::sync::mpsc::Sender<LogEvent>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    start_stream_with(
        view,
        cx,
        capacity,
        move |view: &mut LogsView, event| view.apply_from(&source, event),
        produce,
    )
}

fn start_stream_with<F, Fut>(
    view: gpui_kit::Entity<LogsView>,
    cx: &mut gpui_kit::App,
    capacity: usize,
    apply: impl Fn(&mut LogsView, LogEvent) + 'static,
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
                apply(view, event);
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

/// Which logs to stream: one container of one pod, its current instance or -
/// `previous` - its last-terminated one (`k9s-remaining-keybindings` 5).
/// `context_name` names a failure only; the client carries the context.
/// `tail_lines` starts at the last that many lines rather than the first
/// line - what a panel streaming many containers at once asks for (#150).
#[derive(Clone, Debug, PartialEq)]
pub struct LogTarget {
    pub namespace: String,
    pub pod_name: String,
    pub container: String,
    pub context_name: String,
    pub previous: bool,
    pub tail_lines: Option<i64>,
}

/// The log request for `target`: the current instance followed as it grows, or
/// the previous one's log, which is complete, read to its end.
pub(super) fn log_params(target: &LogTarget) -> kube::api::LogParams {
    kube::api::LogParams {
        container: Some(target.container.clone()),
        follow: !target.previous,
        previous: target.previous,
        tail_lines: target.tail_lines,
        ..Default::default()
    }
}

/// Streams `target`'s logs on `client`, line by line, until the stream ends or
/// the returned `Task` is dropped. A failure
/// to start the stream (e.g. the container hasn't started yet) reports
/// through the same channel as [`LogEvent::RequestFailed`] rather than
/// erroring the caller, matching [`LogsView`]'s own terminal-state handling.
/// `context_name` names the failure only - `client` already carries the
/// context to stream from.
pub fn stream_container_logs(
    client: kube::Client,
    target: LogTarget,
    view: gpui_kit::Entity<LogsView>,
    cx: &mut gpui_kit::App,
) -> gpui_kit::Task<()> {
    start_stream(view, cx, LOG_CHANNEL_CAPACITY, move |tx| {
        produce_container_logs(client, target, tx)
    })
}

/// How many log events may wait for the panel to drain them.
pub(super) const LOG_CHANNEL_CAPACITY: usize = 64;

/// The producer behind every container log stream: reads `target`'s logs on
/// `client` and sends them to `tx` as [`LogEvent`]s, ending with
/// [`LogEvent::Ended`] or a failure, or as soon as `tx`'s receiver is gone.
pub(super) async fn produce_container_logs(
    client: kube::Client,
    target: LogTarget,
    tx: tokio::sync::mpsc::Sender<LogEvent>,
) {
    use futures_util::{AsyncBufReadExt, StreamExt};
    use k8s_openapi::api::core::v1::Pod;
    use kube::Api;

    let api: Api<Pod> = Api::namespaced(client, &target.namespace);
    let LogTarget {
        namespace,
        pod_name,
        context_name,
        previous,
        ..
    } = target.clone();
    let stream = match api.log_stream(&pod_name, &log_params(&target)).await {
        Ok(stream) => stream,
        // The API's answer for a container that never restarted.
        Err(kube::Error::Api(status)) if previous && status.code == 400 => {
            let _ = tx.send(LogEvent::NoPreviousInstance).await;
            return;
        }
        Err(error) => {
            let message = describe_log_stream_error(&error, &pod_name, &namespace, &context_name);
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
}

#[cfg(test)]
mod tests;
