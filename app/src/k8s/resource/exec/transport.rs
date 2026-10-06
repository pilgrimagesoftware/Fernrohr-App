//! The shell's terminal transport (`embedded-exec-terminal` decision 1): a
//! kube exec with a TTY, bridged to `gpui-terminal`.
//!
//! The exec runs on the app's tokio runtime. Its stdout goes straight into the
//! terminal's sink, which is `Send` and wakes the view itself, so output never
//! touches the UI thread on the way in. Input and resizes go the other way
//! through channels, so [`Transport`]'s methods - called on the UI thread with
//! the terminal's lock held - only queue and never wait on the network. A
//! resize is debounced: a drag that resizes the panel many times sends only
//! the size it ends at.
//!
//! When the shell ends, the sink hears the exit (so the view stops taking
//! input) and the panel hears why, through [`ExecEnd`]. Dropping the
//! transport - the panel closing - aborts the task, which closes stdin and
//! stops the reader.
//!
//! [`pump`] is generic over the streams, so a test drives it with in-memory
//! pipes instead of a cluster.

use gpui_terminal::{ExitReport, GridSize, Result, TerminalSink, Transport};
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Status;
use kube::Api;
use kube::api::{AttachParams, TerminalSize};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::sync::{mpsc, watch};

use super::panel::ExecTarget;

/// The shell the session starts: bash where the image has it, else sh.
pub(super) const SHELL: [&str; 3] = [
    "/bin/sh",
    "-c",
    "command -v bash >/dev/null 2>&1 && exec bash || exec sh",
];

/// How a session ended, for the panel's notice.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExecEnd {
    /// The shell's exit code, when the cluster reported one.
    pub(crate) code: Option<i32>,
    /// Why it ended, when that is more than an exit: the exec refused, the
    /// connection dropped.
    pub(crate) reason: Option<String>,
}

/// A running exec, as the terminal sees it.
pub(crate) struct ExecTransport {
    input: mpsc::Sender<Vec<u8>>,
    resize: watch::Sender<Option<GridSize>>,
    task: tokio::task::JoinHandle<()>,
}

impl ExecTransport {
    /// Opens a shell in `target` and pumps it on `runtime`, feeding `sink`
    /// and reporting the end to `ended`.
    pub(crate) fn spawn(
        runtime: &tokio::runtime::Handle,
        client: kube::Client,
        target: ExecTarget,
        sink: TerminalSink,
        ended: mpsc::Sender<ExecEnd>,
    ) -> Self {
        let (input, input_rx) = mpsc::channel(crate::consts::EXEC_INPUT_QUEUE);
        let (resize, resize_rx) = watch::channel(None);
        let task = runtime.spawn(run(client, target, input_rx, resize_rx, sink, ended));
        Self {
            input,
            resize,
            task,
        }
    }

    /// A transport over `task` and its channels - a test's stand-in session.
    #[cfg(test)]
    pub(crate) fn with_task(
        input: mpsc::Sender<Vec<u8>>,
        resize: watch::Sender<Option<GridSize>>,
        task: tokio::task::JoinHandle<()>,
    ) -> Self {
        Self {
            input,
            resize,
            task,
        }
    }
}

impl Transport for ExecTransport {
    /// Queues `bytes` for stdin. A full queue - the shell not reading -
    /// refuses rather than blocks; a closed one means the session is over.
    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.input
            .try_send(bytes.to_vec())
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => gpui_terminal::Error::transport(
                    "the shell isn't reading its input; try again in a moment",
                ),
                mpsc::error::TrySendError::Closed(_) => gpui_terminal::Error::Closed,
            })
    }

    /// Records the size; the pump sends the last one once resizing settles.
    fn resize(&mut self, size: GridSize) -> Result<()> {
        self.resize.send_replace(Some(size));
        Ok(())
    }

    fn terminate(&mut self) -> Result<()> {
        self.task.abort();
        Ok(())
    }
}

impl Drop for ExecTransport {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Opens the exec and pumps it until it ends.
async fn run(
    client: kube::Client,
    target: ExecTarget,
    input: mpsc::Receiver<Vec<u8>>,
    resize: watch::Receiver<Option<GridSize>>,
    sink: TerminalSink,
    ended: mpsc::Sender<ExecEnd>,
) {
    let api: Api<Pod> = Api::namespaced(client, &target.namespace);
    let params = AttachParams::default()
        .container(target.container)
        .stdin(true)
        .stdout(true)
        // With a TTY the container's stderr arrives on stdout.
        .stderr(false)
        .tty(true);
    let mut attached = match api.exec(&target.pod, SHELL, &params).await {
        Ok(attached) => attached,
        Err(error) => {
            let reason = crate::k8s::error::describe(&error);
            finish(&sink, &ended, None, Some(reason)).await;
            return;
        }
    };
    let status = attached.take_status();
    let mut sizes = attached.terminal_size();
    let send_size = move |size: TerminalSize| {
        if let Some(sizes) = &mut sizes {
            // Full only if the cluster stopped reading, when there is no one
            // left to tell.
            let _ = sizes.try_send(size);
        }
    };
    pump(
        attached.stdout(),
        attached.stdin(),
        input,
        resize,
        send_size,
        &sink,
    )
    .await;
    finish_session(status, crate::consts::EXEC_STATUS_TIMEOUT, &sink, &ended).await;
}

/// Ends the session once its output has: waits up to `limit` for the exit
/// status, then reports the exit - or, with no status in time, a lost
/// connection - to the terminal and the panel. Unbounded, a connection that
/// dropped without a status would leave the panel "running" over a frozen
/// screen.
pub(super) async fn finish_session(
    status: Option<impl std::future::Future<Output = Option<Status>>>,
    limit: std::time::Duration,
    sink: &TerminalSink,
    ended: &mpsc::Sender<ExecEnd>,
) {
    let status = match status {
        Some(status) => tokio::time::timeout(limit, status)
            .await
            .unwrap_or_else(|_| {
                log::warn!("the shell's exit status didn't arrive; treating it as lost");
                None
            }),
        None => None,
    };
    let (code, reason) = exit_of(status.as_ref());
    finish(sink, ended, code, reason).await;
}

/// Tells the terminal and the panel the session is over.
async fn finish(
    sink: &TerminalSink,
    ended: &mpsc::Sender<ExecEnd>,
    code: Option<i32>,
    reason: Option<String>,
) {
    sink.exited(ExitReport::new(code));
    let _ = ended.send(ExecEnd { code, reason }).await;
}

/// Pumps `stdout` into `sink`, `input` into `stdin`, and the settled size to
/// `send_size`, until stdout ends. Input closing closes stdin.
pub(super) async fn pump<O, I>(
    stdout: Option<O>,
    mut stdin: Option<I>,
    mut input: mpsc::Receiver<Vec<u8>>,
    mut resize: watch::Receiver<Option<GridSize>>,
    mut send_size: impl FnMut(TerminalSize),
    sink: &TerminalSink,
) where
    O: AsyncRead + Unpin,
    I: AsyncWrite + Unpin,
{
    let Some(mut stdout) = stdout else {
        return;
    };
    let mut buffer = [0u8; 4096];
    let mut input_open = true;
    let mut resizing = true;
    // A size waiting for resizing to settle, and when it will have. Only a
    // new size moves the deadline - output in between must not, or a busy
    // program would keep its size from ever being sent.
    let mut pending: Option<(GridSize, tokio::time::Instant)> = None;
    loop {
        let settle = async {
            match pending {
                Some((_, settled)) => tokio::time::sleep_until(settled).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            read = stdout.read(&mut buffer) => match read {
                Ok(0) | Err(_) => return,
                Ok(read) => sink.output(&buffer[..read]),
            },
            bytes = input.recv(), if input_open => match (bytes, stdin.as_mut()) {
                (Some(bytes), Some(writer)) => {
                    if writer.write_all(&bytes).await.is_err() || writer.flush().await.is_err() {
                        stdin = None;
                    }
                }
                (Some(_), None) => {}
                (None, _) => {
                    input_open = false;
                    stdin = None;
                }
            },
            changed = resize.changed(), if resizing => match changed {
                Ok(()) => {
                    let settled = tokio::time::Instant::now() + crate::consts::EXEC_RESIZE_DEBOUNCE;
                    pending = (*resize.borrow_and_update()).map(|size| (size, settled));
                }
                Err(_) => resizing = false,
            },
            () = settle => {
                if let Some((size, _)) = pending.take() {
                    send_size(terminal_size(size));
                }
            }
        }
    }
}

/// A grid size as the exec's resize channel takes it.
fn terminal_size(size: GridSize) -> TerminalSize {
    let clamp = |value: usize| u16::try_from(value).unwrap_or(u16::MAX);
    TerminalSize {
        width: clamp(size.columns),
        height: clamp(size.rows),
    }
}

/// The exit code and reason the exec's final status reports. Success is code
/// 0; a shell that exited non-zero carries its code in an `ExitCode` cause;
/// any other failure is a reason without a code. No status at all - the
/// connection dropped - is a reason too.
pub(super) fn exit_of(status: Option<&Status>) -> (Option<i32>, Option<String>) {
    let Some(status) = status else {
        return (None, Some("The connection to the shell was lost.".into()));
    };
    if status.status.as_deref() == Some("Success") {
        return (Some(0), None);
    }
    let code = status
        .details
        .as_ref()
        .and_then(|details| details.causes.as_deref())
        .and_then(|causes| {
            causes
                .iter()
                .find(|cause| cause.reason.as_deref() == Some("ExitCode"))
        })
        .and_then(|cause| cause.message.as_deref()?.parse().ok());
    match code {
        Some(code) => (Some(code), None),
        None => (
            None,
            Some(
                status
                    .message
                    .clone()
                    .filter(|message| !message.is_empty())
                    .unwrap_or_else(|| "The shell ended with an error.".into()),
            ),
        ),
    }
}

#[cfg(test)]
mod tests;
