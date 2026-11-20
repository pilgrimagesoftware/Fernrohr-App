//! The app's side of the endpoint: [`start`] binds it at launch, and
//! [`serve`] answers each connection on the tokio runtime - the peer's uid,
//! then the token handshake, then one request. A request is either a session
//! opening, which keeps its connection for the MCP client's life
//! (`mcp-connections` design decision 3), or a call naming a live session.
//!
//! A tool call runs until it finishes or the adapter hangs up, whichever comes
//! first; hanging up drops the call's future, which is how a disconnected
//! client cancels a call (design.md Risks). Every request is logged by name,
//! outcome code and duration only - never its arguments, result or the token
//! (`agent-mcp`: Credential and error safety).

use super::approval::ApprovalGate;
use super::endpoint::{BindError, Endpoint, EndpointFiles, EndpointPaths};
use super::error::ToolError;
use super::error::Unavailability;
use super::foreground::Foreground;
use super::frame::{self, FrameError};
use super::protocol::{
    EndpointToken, Hello, HelloReply, PROTOCOL_VERSION, Rejection, Reply, Request,
};
use super::session::Sessions;
use super::tools::{ToolContext, ToolRegistry};
use crate::consts::{
    MCP_HANDSHAKE_TIMEOUT, MCP_MAX_CONNECTIONS, MCP_MAX_REPLY_BYTES, MCP_MAX_REQUEST_BYTES,
    MCP_MAX_SESSIONS, MCP_SESSION_GRACE,
};
use crate::k8s::cluster::session::{AgentId, LiveAgents};
use gpui_kit::{App, Global};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::net::unix::OwnedReadHalf;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
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
    /// The MCP sessions open now.
    pub(super) sessions: Arc<Sessions>,
    /// How many more sessions may open: [`MCP_MAX_SESSIONS`].
    pub(super) session_slots: Arc<Semaphore>,
    /// How long an ended session's holds linger: [`MCP_SESSION_GRACE`] but in
    /// tests.
    pub(super) session_grace: Duration,
}

impl ServerState {
    /// A served endpoint's state, with no session open yet.
    pub(super) fn new(
        registry: ToolRegistry,
        token: EndpointToken,
        owner_uid: u32,
        context: ToolContext,
    ) -> Self {
        Self {
            registry,
            token,
            owner_uid,
            context,
            sessions: Arc::default(),
            session_slots: Arc::new(Semaphore::new(MCP_MAX_SESSIONS)),
            session_grace: MCP_SESSION_GRACE,
        }
    }
}

/// The endpoint this app serves, from [`start`] until [`stop`].
struct McpEndpoint {
    files: EndpointFiles,
    task: JoinHandle<()>,
    /// Kept here too, so [`stop`] can end every session: aborting the serving
    /// task drops their connections without running their close path.
    sessions: Arc<Sessions>,
}

impl Global for McpEndpoint {}

impl Drop for McpEndpoint {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Marks that the quit hook removing the endpoint's files is registered, so
/// starting the endpoint again after [`stop`] doesn't register a second.
struct QuitHook;

impl Global for QuitHook {}

/// Binds the app's endpoint and serves `registry` on it until [`stop`] or the
/// app quits, when the socket and token are removed. A no-op while it already
/// runs. Without an endpoint - another Fernrohr already serves it, or the files
/// couldn't be written - the app runs as usual and `fernrohr mcp` reaches the
/// other instance or none.
pub(super) fn start(cx: &mut App, registry: ToolRegistry) {
    if running(cx) {
        return;
    }
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
    let context = {
        let foreground = Foreground::spawn_on(cx);
        ToolContext {
            approvals: ApprovalGate::ui(foreground.clone()),
            foreground,
            kubeconfig: None,
            tunnels: None,
            connect_settle: crate::consts::MCP_CONNECT_SETTLE,
            caller: None,
        }
    };
    let state = Arc::new(ServerState::new(
        registry,
        files.token.clone(),
        endpoint.owner_uid,
        context,
    ));
    let sessions = Arc::clone(&state.sessions);
    let task = handle.spawn(serve(endpoint.listener, state));
    cx.set_global(McpEndpoint {
        files,
        task,
        sessions,
    });
    if cx.has_global::<QuitHook>() {
        return;
    }
    cx.set_global(QuitHook);
    // GPUI never drops app globals on quit (see `util::pidfile`), so the
    // files are removed here, synchronously, before the process exits.
    cx.on_app_quit(|cx| {
        stop(cx);
        async {}
    })
    .detach();
}

/// Whether this app serves the endpoint now.
pub(super) fn running(cx: &App) -> bool {
    cx.has_global::<McpEndpoint>()
}

/// Stops serving the endpoint, if this app serves it: every connection is
/// dropped, every session ends - its holds linger for the grace period, then
/// go - and the socket and token are removed, so `fernrohr mcp` finds no app
/// until [`start`] binds it again.
pub(super) fn stop(cx: &mut App) {
    if !running(cx) {
        return;
    }
    let endpoint = cx.remove_global::<McpEndpoint>();
    endpoint.files.remove();
    let sessions = Arc::clone(&endpoint.sessions);
    // Dropping it aborts the serving task, and with it every connection.
    drop(endpoint);
    end_every_session(cx, &sessions, MCP_SESSION_GRACE);
}

/// What stopping the endpoint does to its sessions (`agent-mcp` "Stop serving
/// with the last window"): every identifier stops working, and every live
/// agent ends - its holds linger for `grace`, then go. Aborting the serving
/// task skips each session's own close path, so this stands in for it. The
/// main thread's live set is the source, not the endpoint's session list, so an
/// agent whose own end was cut short mid-way still ends (review of App#434, F2).
fn end_every_session(cx: &mut App, sessions: &Sessions, grace: Duration) {
    sessions.close_all();
    LiveAgents::end_all(cx, grace);
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
                        serve_connection(stream, &state, slot).await;
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

/// One connection, start to finish. `slot` is its place among
/// [`MCP_MAX_CONNECTIONS`]; a session gives it back once it holds a session
/// slot instead.
async fn serve_connection(stream: UnixStream, state: &ServerState, slot: OwnedSemaphorePermit) {
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
        Request::OpenSession { client_name } => {
            return serve_session(client_name, reader, writer, state, slot).await;
        }
        Request::ListTools { session } => (
            "tools/list".to_string(),
            match state.sessions.caller(&session) {
                Some(_) => Reply::Tools {
                    tools: state.registry.specs(),
                },
                None => no_session(),
            },
        ),
        Request::CallTool {
            session,
            name,
            arguments,
        } => {
            let Some(caller) = state.sessions.caller(&session) else {
                let reply = no_session();
                log_request(&name, outcome(&reply), started);
                let _ = frame::write(&mut writer, &reply, MCP_MAX_REPLY_BYTES).await;
                return;
            };
            let mut context = state.context.clone();
            context.caller = Some(caller);
            let call = state.registry.call(&name, arguments, context);
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

/// A session's connection (`mcp-connections` design decision 3): opens the
/// session, hands the adapter its identifier, and holds the connection until
/// the adapter closes it - which ends the session.
async fn serve_session(
    client_name: Option<String>,
    mut reader: OwnedReadHalf,
    mut writer: tokio::net::unix::OwnedWriteHalf,
    state: &ServerState,
    connection_slot: OwnedSemaphorePermit,
) {
    let Ok(session_slot) = Arc::clone(&state.session_slots).try_acquire_owned() else {
        log::warn!(target: LOG_TARGET, "refused a session: all session slots busy");
        let _ = frame::write(
            &mut writer,
            &Reply::Refused {
                error: ToolError::Unavailable {
                    reason: Unavailability::TooManySessions,
                },
            },
            MCP_MAX_REPLY_BYTES,
        )
        .await;
        return;
    };
    // A session lasts the client's lifetime; it mustn't keep a request slot.
    drop(connection_slot);
    let (id, caller) = match state.sessions.open(client_name) {
        Ok(opened) => opened,
        Err(error) => {
            log::warn!(target: LOG_TARGET, "could not open a session: {error}");
            let _ = frame::write(
                &mut writer,
                &Reply::Refused {
                    error: ToolError::Internal,
                },
                MCP_MAX_REPLY_BYTES,
            )
            .await;
            return;
        }
    };
    let agent = caller.agent;
    // Live on the main thread before the identifier is handed out, so nothing
    // the session asks for can find it not live.
    let name = caller.client_name.clone();
    // Fails only with no main thread (the app quitting): the session is still
    // served, and since its agent never became live, `ClusterRegistry::hold`
    // refuses it anything to hold.
    let _ = state
        .context
        .foreground
        .run(move |cx| LiveAgents::open(cx, agent, name))
        .await;
    // Never the identifier, nor the self-reported name: the in-app handle only.
    log::info!(target: LOG_TARGET, "session opened agent={agent:?}");
    let opened = frame::write(
        &mut writer,
        &Reply::Session { session: id },
        MCP_MAX_REPLY_BYTES,
    )
    .await;
    if opened.is_ok() {
        hang_up(&mut reader).await;
    }
    end_session(state, agent).await;
    log::info!(target: LOG_TARGET, "session ended agent={agent:?}");
    drop(session_slot);
}

/// Ends `agent`'s session: its identifier stops working now, and its holds
/// linger on the main thread for the grace period. Awaited rather than posted,
/// so a full main-thread queue delays the hand-off instead of dropping it and
/// leaving the holds in place for good. Fails only when the app is quitting,
/// which drops every hold anyway.
async fn end_session(state: &ServerState, agent: AgentId) {
    state.sessions.close(agent);
    let grace = state.session_grace;
    let _ = state
        .context
        .foreground
        .run(move |cx| LiveAgents::end(cx, agent, grace))
        .await;
}

/// The refusal for a request naming no live session.
fn no_session() -> Reply {
    Reply::Refused {
        error: ToolError::Unavailable {
            reason: Unavailability::NoSession,
        },
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
        Reply::Session { .. } | Reply::Tools { .. } | Reply::Called { result: Ok(_) } => "ok",
        Reply::Called { result: Err(error) } | Reply::Refused { error } => error.code(),
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
