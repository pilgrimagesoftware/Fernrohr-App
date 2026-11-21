//! The adapter's side of the endpoint.
//!
//! The adapter holds one MCP session with the app (`mcp-connections` design
//! decision 3): a connection opened on the first request, carrying the name the
//! MCP client reported, and kept open for the adapter's life - closing it, by
//! exiting, ends the session. Each request is then its own connection naming
//! that session, so an app restarted mid-session is picked up by the next
//! request, and dropping the request's future hangs up, which cancels it in
//! the app.
//!
//! If the app says the session is gone (it restarted, or stopped serving MCP),
//! the adapter opens a new one and retries once. A new session holds nothing
//! and needs the user's approval again (Security 2), so reopening grants no
//! access.
//!
//! Every failure comes back as a [`ToolError`], chiefly
//! [`ToolError::Unavailable`] when no app answers.

use super::endpoint::EndpointPaths;
use super::error::{ToolError, Unavailability};
use super::frame::{self, FrameError};
use super::protocol::{
    EndpointToken, Hello, HelloReply, PROTOCOL_VERSION, Rejection, Reply, Request,
};
use super::session::SessionId;
use crate::consts::{MCP_MAX_REPLY_BYTES, MCP_MAX_REQUEST_BYTES};
use serde_json::{Map, Value};
use std::sync::Arc;
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

/// The largest [`HelloReply`] the adapter reads.
const HELLO_REPLY_LIMIT: usize = 1024;

/// What the adapter asks the app for; [`AppClient::request`] adds the session.
#[derive(Debug, Clone)]
pub(super) enum Call {
    ListTools,
    CallTool {
        name: String,
        arguments: Map<String, Value>,
    },
}

impl Call {
    fn with_session(&self, session: SessionId) -> Request {
        match self.clone() {
            Self::ListTools => Request::ListTools { session },
            Self::CallTool { name, arguments } => Request::CallTool {
                session,
                name,
                arguments,
            },
        }
    }
}

/// The open session: its identifier, and the connection whose closing ends it.
struct OpenSession {
    id: SessionId,
    _link: (OwnedReadHalf, OwnedWriteHalf),
}

/// A handle on the app's endpoint at fixed paths, and this adapter's session.
#[derive(Clone)]
pub(super) struct AppClient {
    paths: EndpointPaths,
    /// A tokio mutex: opening a session awaits its handshake under the lock, so
    /// two first requests can't open two sessions.
    session: Arc<tokio::sync::Mutex<Option<OpenSession>>>,
}

impl AppClient {
    pub(super) fn new(paths: EndpointPaths) -> Self {
        Self {
            paths,
            session: Arc::default(),
        }
    }

    /// Sends `call` in this adapter's session, opening the session first if it
    /// isn't open - reporting `client_name` as the MCP client's name - and
    /// returns the app's reply.
    pub(super) async fn request(
        &self,
        call: &Call,
        client_name: Option<&str>,
    ) -> Result<Reply, ToolError> {
        let session = self.session(client_name).await?;
        let reply = self.exchange(&call.with_session(session.clone())).await?;
        if !is_no_session(&reply) {
            return Ok(reply);
        }
        self.forget(&session).await;
        let session = self.session(client_name).await?;
        self.exchange(&call.with_session(session)).await
    }

    /// The open session's identifier, opening one if there's none.
    async fn session(&self, client_name: Option<&str>) -> Result<SessionId, ToolError> {
        let mut slot = self.session.lock().await;
        if let Some(open) = slot.as_ref() {
            return Ok(open.id.clone());
        }
        let (mut reader, mut writer) = self.connect().await?;
        let open = Request::OpenSession {
            client_name: client_name.map(str::to_string),
        };
        send(&mut writer, &open).await?;
        match receive(&mut reader).await? {
            Reply::Session { session } => {
                *slot = Some(OpenSession {
                    id: session.clone(),
                    _link: (reader, writer),
                });
                Ok(session)
            }
            Reply::Refused { error } => Err(error),
            Reply::Tools { .. } | Reply::Called { .. } => Err(ToolError::Internal),
        }
    }

    /// Drops the session `stale`, if it's still the open one, closing its link.
    async fn forget(&self, stale: &SessionId) {
        let mut slot = self.session.lock().await;
        if slot.as_ref().is_some_and(|open| &open.id == stale) {
            *slot = None;
        }
    }

    /// One request on its own connection, and its reply.
    async fn exchange(&self, request: &Request) -> Result<Reply, ToolError> {
        let (mut reader, mut writer) = self.connect().await?;
        send(&mut writer, request).await?;
        receive(&mut reader).await
    }

    /// A connection past the handshake. A refused token is retried once with
    /// the token re-read: the app may have restarted, and rotated it, between
    /// reading and connecting.
    async fn connect(&self) -> Result<(OwnedReadHalf, OwnedWriteHalf), ToolError> {
        match self.handshake().await {
            Err(Attempt::TokenRefused) => match self.handshake().await {
                Err(Attempt::TokenRefused) => Err(ToolError::Unavailable {
                    reason: Unavailability::Unauthorized,
                }),
                result => result.map_err(Attempt::into_tool_error),
            },
            result => result.map_err(Attempt::into_tool_error),
        }
    }

    async fn handshake(&self) -> Result<(OwnedReadHalf, OwnedWriteHalf), Attempt> {
        let not_running = |_| {
            Attempt::Failed(ToolError::Unavailable {
                reason: Unavailability::NotRunning,
            })
        };
        let stream = UnixStream::connect(&self.paths.socket)
            .await
            .map_err(not_running)?;
        let token = EndpointToken::read(&self.paths.token).map_err(not_running)?;
        let (mut reader, mut writer) = stream.into_split();

        let hello = Hello {
            protocol: PROTOCOL_VERSION,
            token,
        };
        frame::write(&mut writer, &hello, MCP_MAX_REQUEST_BYTES)
            .await
            .map_err(|_| unavailable(Unavailability::NotRunning))?;
        match frame::read::<HelloReply>(&mut reader, HELLO_REPLY_LIMIT).await {
            Ok(HelloReply::Accepted) => Ok((reader, writer)),
            Ok(HelloReply::Rejected {
                reason: Rejection::Token,
            }) => Err(Attempt::TokenRefused),
            Ok(HelloReply::Rejected {
                reason: Rejection::Protocol,
            }) => Err(unavailable(Unavailability::Incompatible)),
            // A reply this build can't read is another build's.
            Err(FrameError::Malformed | FrameError::TooLarge) => {
                Err(unavailable(Unavailability::Incompatible))
            }
            // Hung up mid-handshake: quitting, or refused this uid.
            Err(FrameError::Closed | FrameError::Io(_)) => {
                Err(unavailable(Unavailability::NotRunning))
            }
        }
    }
}

async fn send(writer: &mut OwnedWriteHalf, request: &Request) -> Result<(), ToolError> {
    match frame::write(writer, request, MCP_MAX_REQUEST_BYTES).await {
        Ok(()) => Ok(()),
        Err(FrameError::TooLarge) => Err(ToolError::InvalidArguments {
            message: format!(
                "the arguments are larger than the {MCP_MAX_REQUEST_BYTES}-byte limit"
            ),
        }),
        Err(_) => Err(ToolError::Unavailable {
            reason: Unavailability::NotRunning,
        }),
    }
}

async fn receive(reader: &mut OwnedReadHalf) -> Result<Reply, ToolError> {
    frame::read::<Reply>(reader, MCP_MAX_REPLY_BYTES)
        .await
        .map_err(|error| match error {
            FrameError::TooLarge => ToolError::ResultTooLarge {
                limit: MCP_MAX_REPLY_BYTES,
            },
            FrameError::Closed | FrameError::Io(_) | FrameError::Malformed => ToolError::Internal,
        })
}

/// Whether the app refused a request because its session is gone.
fn is_no_session(reply: &Reply) -> bool {
    matches!(
        reply,
        Reply::Refused {
            error: ToolError::Unavailable {
                reason: Unavailability::NoSession
            }
        }
    )
}

/// How one connection attempt ended short of the handshake.
enum Attempt {
    /// The app refused the token; worth one retry.
    TokenRefused,
    Failed(ToolError),
}

impl Attempt {
    fn into_tool_error(self) -> ToolError {
        match self {
            Self::TokenRefused => ToolError::Unavailable {
                reason: Unavailability::Unauthorized,
            },
            Self::Failed(error) => error,
        }
    }
}

fn unavailable(reason: Unavailability) -> Attempt {
    Attempt::Failed(ToolError::Unavailable { reason })
}

#[cfg(test)]
mod tests;
