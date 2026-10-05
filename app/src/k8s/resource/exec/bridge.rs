//! The exec's plumbing: the attach call, and the pump between its streams and
//! the app's channels - stdout and stderr out as [`ExecEvent::Output`], the
//! user's input in to stdin - until the remote side ends.
//!
//! [`bridge`] is generic over the streams so a test can drive it with in-memory
//! pipes instead of a cluster.

use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::api::AttachParams;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::sync::mpsc;

/// The shell the session starts: bash where the image has it, else sh.
pub(super) const SHELL: [&str; 3] = [
    "/bin/sh",
    "-c",
    "command -v bash >/dev/null 2>&1 && exec bash || exec sh",
];

/// What the session reports to the panel.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ExecEvent {
    /// Text the container wrote, stdout and stderr alike.
    Output(String),
    /// The session is over - the shell exited, the container stopped, or the
    /// exec was refused - with the reason, when there is one.
    Ended(Option<String>),
}

/// Opens a shell in `container` of `pod` and pumps it until it ends, reporting
/// to `events` and reading the user's input from `input`.
pub(super) async fn run(
    client: kube::Client,
    namespace: String,
    pod: String,
    container: String,
    input: mpsc::Receiver<Vec<u8>>,
    events: mpsc::Sender<ExecEvent>,
) {
    let api: Api<Pod> = Api::namespaced(client, &namespace);
    let params = AttachParams::default()
        .container(container)
        .stdin(true)
        .stdout(true)
        .stderr(true)
        .tty(false);
    let mut attached = match api.exec(&pod, SHELL, &params).await {
        Ok(attached) => attached,
        Err(error) => {
            let _ = events
                .send(ExecEvent::Ended(Some(crate::k8s::error::describe(&error))))
                .await;
            return;
        }
    };
    let status = attached.take_status();
    bridge(
        attached.stdout(),
        attached.stderr(),
        attached.stdin(),
        input,
        events.clone(),
    )
    .await;
    let reason = match status {
        Some(status) => status.await.and_then(|status| {
            (status.status.as_deref() == Some("Failure")).then(|| {
                status
                    .message
                    .unwrap_or_else(|| "The shell exited with an error.".into())
            })
        }),
        None => None,
    };
    let _ = events.send(ExecEvent::Ended(reason)).await;
}

/// Pumps `stdout` and `stderr` into `events` and `input` into `stdin` until both
/// output streams have ended. Input closing (the panel went away) closes stdin,
/// which ends a shell reading it.
pub(super) async fn bridge<O, E, I>(
    stdout: Option<O>,
    stderr: Option<E>,
    mut stdin: Option<I>,
    mut input: mpsc::Receiver<Vec<u8>>,
    events: mpsc::Sender<ExecEvent>,
) where
    O: AsyncRead + Unpin,
    E: AsyncRead + Unpin,
    I: AsyncWrite + Unpin,
{
    let (mut stdout, mut stderr) = (stdout, stderr);
    let (mut out_buffer, mut err_buffer) = ([0u8; 4096], [0u8; 4096]);
    let mut input_open = true;
    while stdout.is_some() || stderr.is_some() {
        tokio::select! {
            read = read_some(&mut stdout, &mut out_buffer) => {
                if !forward(read, &mut stdout, &out_buffer, &events).await {
                    return;
                }
            }
            read = read_some(&mut stderr, &mut err_buffer) => {
                if !forward(read, &mut stderr, &err_buffer, &events).await {
                    return;
                }
            }
            line = input.recv(), if input_open => match (line, stdin.as_mut()) {
                (Some(line), Some(writer)) => {
                    if writer.write_all(&line).await.is_err() || writer.flush().await.is_err() {
                        stdin = None;
                    }
                }
                (Some(_), None) => {}
                (None, _) => {
                    input_open = false;
                    stdin = None;
                }
            },
        }
    }
}

/// Reads what `stream` has, or waits forever once it has ended - so a finished
/// stream drops out of the `select!` without ending the loop.
async fn read_some<R: AsyncRead + Unpin>(
    stream: &mut Option<R>,
    buffer: &mut [u8],
) -> std::io::Result<usize> {
    match stream {
        Some(stream) => stream.read(buffer).await,
        None => std::future::pending().await,
    }
}

/// Passes `read` bytes of `buffer` on as output, or marks `stream` ended on
/// EOF or an error. Whether the panel is still listening.
async fn forward<R>(
    read: std::io::Result<usize>,
    stream: &mut Option<R>,
    buffer: &[u8],
    events: &mpsc::Sender<ExecEvent>,
) -> bool {
    match read {
        Ok(0) | Err(_) => {
            *stream = None;
            true
        }
        Ok(read) => {
            let text = String::from_utf8_lossy(&buffer[..read]).into_owned();
            events.send(ExecEvent::Output(text)).await.is_ok()
        }
    }
}

#[cfg(test)]
mod tests;
