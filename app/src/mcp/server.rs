//! The app's side of the endpoint: [`start`] binds it at launch, and
//! [`serve`] answers each connection on the tokio runtime - the peer's uid,
//! then the token handshake, then one request.
//!
//! A tool call runs until it finishes or the adapter hangs up, whichever comes
//! first; hanging up drops the call's future, which is how a disconnected
//! client cancels a call (design.md Risks). Every request is logged by name,
//! outcome code and duration only - never its arguments, result or the token
//! (`agent-mcp`: Credential and error safety).

use super::approval::ApprovalGate;
use super::endpoint::{BindError, Endpoint, EndpointFiles, EndpointPaths};
use super::error::ToolError;
use super::foreground::Foreground;
use super::frame::{self, FrameError};
use super::protocol::{
    EndpointToken, Hello, HelloReply, PROTOCOL_VERSION, Rejection, Reply, Request,
};
use super::tools::{ToolContext, ToolRegistry};
use crate::consts::{
    MCP_HANDSHAKE_TIMEOUT, MCP_MAX_CONNECTIONS, MCP_MAX_REPLY_BYTES, MCP_MAX_REQUEST_BYTES,
};
use gpui_kit::{App, Global};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::net::unix::OwnedReadHalf;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Semaphore;
use tokio::task::{JoinHandle, JoinSet};

/// The `log` target every endpoint line is written under.
const LOG_TARGET: &str = "fernrohr::mcp";

/// A [`Hello`] is a version and a 64-character token; nothing legitimate
/// comes near this.
const HELLO_LIMIT: usize = 4096;

/// What one served endpoint needs: its tools, and who may call them.
pub(super) struct ServerState {
    pub(super) registry: ToolRegistry,
    pub(super) token: EndpointToken,
    pub(super) owner_uid: u32,
    pub(super) context: ToolContext,
}

/// The running endpoint, kept for the app's lifetime.
struct McpEndpoint {
    files: EndpointFiles,
    task: JoinHandle<()>,
}

impl Global for McpEndpoint {}

impl Drop for McpEndpoint {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Binds the app's endpoint and serves `registry` on it until the app quits,
/// when the socket and token are removed. Without an endpoint - another
/// Fernrohr already serves it, or the files couldn't be written - the app runs
/// as usual and `fernrohr mcp` reaches the other instance or none.
pub(super) fn start(cx: &mut App, registry: ToolRegistry) {
    let handle = crate::runtime::handle(cx);
    let bound = {
        let _runtime = handle.enter();
        Endpoint::bind(EndpointPaths::default_location())
    };
    let endpoint = match bound {
        Ok(endpoint) => endpoint,
        Err(BindError::InUse) => {
            log::warn!(target: LOG_TARGET, "another Fernrohr serves the MCP endpoint; not starting one");
            return;
        }
        Err(BindError::Io(error)) => {
            log::warn!(target: LOG_TARGET, "could not start the MCP endpoint: {error}");
            return;
        }
    };
    let files = endpoint.files.clone();
    let state = Arc::new(ServerState {
        registry,
        token: files.token.clone(),
        owner_uid: endpoint.owner_uid,
        context: {
            let foreground = Foreground::spawn_on(cx);
            ToolContext {
                approvals: ApprovalGate::ui(foreground.clone()),
                foreground,
                kubeconfig: None,
                tunnels: None,
                connect_settle: crate::consts::MCP_CONNECT_SETTLE,
            }
        },
    });
    let task = handle.spawn(serve(endpoint.listener, state));
    cx.set_global(McpEndpoint { files, task });
    // GPUI never drops app globals on quit (see `util::pidfile`), so the
    // files are removed here, synchronously, before the process exits.
    cx.on_app_quit(|cx| {
        if let Some(endpoint) = cx.try_global::<McpEndpoint>() {
            endpoint.task.abort();
            endpoint.files.remove();
        }
        async {}
    })
    .detach();
}

/// Accepts connections on `listener` forever, serving at most
/// [`MCP_MAX_CONNECTIONS`] at once. A connection past that is closed at once.
/// Aborting this task aborts every connection it is serving.
pub(super) async fn serve(listener: UnixListener, state: Arc<ServerState>) {
    /// How long to wait after a failed `accept` (out of file descriptors,
    /// say) before trying again, so the loop can't spin.
    const ACCEPT_RETRY: Duration = Duration::from_millis(100);

    let slots = Arc::new(Semaphore::new(MCP_MAX_CONNECTIONS));
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    let Ok(slot) = Arc::clone(&slots).try_acquire_owned() else {
                        log::warn!(target: LOG_TARGET, "refused a connection: all slots busy");
                        continue;
                    };
                    let state = Arc::clone(&state);
                    connections.spawn(async move {
                        serve_connection(stream, &state).await;
                        drop(slot);
                    });
                }
                Err(error) => {
                    log::warn!(target: LOG_TARGET, "accept failed: {error}");
                    tokio::time::sleep(ACCEPT_RETRY).await;
                }
            },
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
}

/// One connection, start to finish.
async fn serve_connection(stream: UnixStream, state: &ServerState) {
    let peer_uid = stream.peer_cred().map(|cred| cred.uid());
    if peer_uid.as_ref().ok() != Some(&state.owner_uid) {
        log::warn!(target: LOG_TARGET, "refused a connection from another user");
        return;
    }
    let (mut reader, mut writer) = stream.into_split();

    let hello = tokio::time::timeout(
        MCP_HANDSHAKE_TIMEOUT,
        frame::read::<Hello>(&mut reader, HELLO_LIMIT),
    )
    .await;
    let verdict = match hello {
        Ok(Ok(hello)) if hello.protocol != PROTOCOL_VERSION => HelloReply::Rejected {
            reason: Rejection::Protocol,
        },
        Ok(Ok(hello)) if hello.token.matches(&state.token) => HelloReply::Accepted,
        // A wrong or stale token, or a hello with none at all.
        Ok(Ok(_) | Err(FrameError::Malformed)) => HelloReply::Rejected {
            reason: Rejection::Token,
        },
        Ok(Err(_)) | Err(_) => return,
    };
    if frame::write(&mut writer, &verdict, HELLO_LIMIT)
        .await
        .is_err()
    {
        return;
    }
    if let HelloReply::Rejected { reason } = verdict {
        log::warn!(target: LOG_TARGET, "refused a handshake: {reason:?}");
        return;
    }

    let request = match frame::read::<Request>(&mut reader, MCP_MAX_REQUEST_BYTES).await {
        Ok(request) => request,
        Err(error) => {
            log::warn!(target: LOG_TARGET, "unreadable request: {error}");
            return;
        }
    };
    let started = Instant::now();
    let (label, reply) = match request {
        Request::ListTools => (
            "tools/list".to_string(),
            Reply::Tools {
                tools: state.registry.specs(),
            },
        ),
        Request::CallTool { name, arguments } => {
            let call = state.registry.call(&name, arguments, state.context.clone());
            let result = tokio::select! {
                result = call => result,
                () = hang_up(&mut reader) => {
                    log_request(&name, "cancelled", started);
                    return;
                }
            };
            (name, Reply::Called { result })
        }
    };

    let sent = match frame::write(&mut writer, &reply, MCP_MAX_REPLY_BYTES).await {
        Err(FrameError::TooLarge) => {
            let too_large = Reply::Called {
                result: Err(ToolError::ResultTooLarge {
                    limit: MCP_MAX_REPLY_BYTES,
                }),
            };
            log_request(&label, "result_too_large", started);
            frame::write(&mut writer, &too_large, MCP_MAX_REPLY_BYTES).await
        }
        sent => {
            log_request(&label, outcome(&reply), started);
            sent
        }
    };
    if let Err(error) = sent {
        log::warn!(target: LOG_TARGET, "could not send a reply: {error}");
    }
}

/// Resolves once the adapter closes its end - after its request it sends
/// nothing more, so any read returning means it is gone.
async fn hang_up(reader: &mut OwnedReadHalf) {
    let mut byte = [0u8; 1];
    let _ = reader.read(&mut byte).await;
}

fn outcome(reply: &Reply) -> &'static str {
    match reply {
        Reply::Tools { .. } | Reply::Called { result: Ok(_) } => "ok",
        Reply::Called { result: Err(error) } => error.code(),
    }
}

/// The one log line per request: what was asked, how it ended, how long it
/// took. `request` is a tool name or `tools/list`; `{:?}` keeps a client's
/// name from writing a line of its own.
fn log_request(request: &str, outcome: &str, started: Instant) {
    log::info!(
        target: LOG_TARGET,
        "request={request:?} outcome={outcome} elapsed_ms={}",
        started.elapsed().as_millis()
    );
}

#[cfg(test)]
mod tests;
