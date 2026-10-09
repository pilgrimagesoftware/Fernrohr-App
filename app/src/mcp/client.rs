//! The adapter's side of the endpoint: [`AppClient::request`] connects to the
//! running app, proves itself with the token, sends one request and reads its
//! reply - a fresh connection each time, so an app restarted mid-session is
//! picked up by the next request, and dropping the request's future hangs up,
//! which cancels it in the app.
//!
//! Every failure comes back as a [`ToolError`], chiefly
//! [`ToolError::Unavailable`] when no app answers.

use super::endpoint::EndpointPaths;
use super::error::{ToolError, Unavailability};
use super::frame::{self, FrameError};
use super::protocol::{
    EndpointToken, Hello, HelloReply, PROTOCOL_VERSION, Rejection, Reply, Request,
};
use crate::consts::{MCP_MAX_REPLY_BYTES, MCP_MAX_REQUEST_BYTES};
use tokio::net::UnixStream;

/// The largest [`HelloReply`] the adapter reads.
const HELLO_REPLY_LIMIT: usize = 1024;

/// A handle on the app's endpoint at fixed paths.
#[derive(Debug, Clone)]
pub(super) struct AppClient {
    paths: EndpointPaths,
}

impl AppClient {
    pub(super) fn new(paths: EndpointPaths) -> Self {
        Self { paths }
    }

    /// Sends `request` and returns the app's reply.
    ///
    /// A refused token is retried once with the token re-read: the app may
    /// have restarted, and rotated it, between reading and connecting.
    pub(super) async fn request(&self, request: &Request) -> Result<Reply, ToolError> {
        match self.attempt(request).await {
            Err(Attempt::TokenRefused) => match self.attempt(request).await {
                Err(Attempt::TokenRefused) => Err(ToolError::Unavailable {
                    reason: Unavailability::Unauthorized,
                }),
                result => result.map_err(Attempt::into_tool_error),
            },
            result => result.map_err(Attempt::into_tool_error),
        }
    }

    async fn attempt(&self, request: &Request) -> Result<Reply, Attempt> {
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
            Ok(HelloReply::Accepted) => {}
            Ok(HelloReply::Rejected {
                reason: Rejection::Token,
            }) => return Err(Attempt::TokenRefused),
            Ok(HelloReply::Rejected {
                reason: Rejection::Protocol,
            }) => return Err(unavailable(Unavailability::Incompatible)),
            // A reply this build can't read is another build's.
            Err(FrameError::Malformed | FrameError::TooLarge) => {
                return Err(unavailable(Unavailability::Incompatible));
            }
            // Hung up mid-handshake: quitting, or refused this uid.
            Err(FrameError::Closed | FrameError::Io(_)) => {
                return Err(unavailable(Unavailability::NotRunning));
            }
        }

        match frame::write(&mut writer, request, MCP_MAX_REQUEST_BYTES).await {
            Ok(()) => {}
            Err(FrameError::TooLarge) => {
                return Err(Attempt::Failed(ToolError::InvalidArguments {
                    message: format!(
                        "the arguments are larger than the {MCP_MAX_REQUEST_BYTES}-byte limit"
                    ),
                }));
            }
            Err(_) => return Err(unavailable(Unavailability::NotRunning)),
        }
        frame::read::<Reply>(&mut reader, MCP_MAX_REPLY_BYTES)
            .await
            .map_err(|error| {
                Attempt::Failed(match error {
                    FrameError::TooLarge => ToolError::ResultTooLarge {
                        limit: MCP_MAX_REPLY_BYTES,
                    },
                    FrameError::Closed | FrameError::Io(_) | FrameError::Malformed => {
                        ToolError::Internal
                    }
                })
            })
    }
}

/// How one connection attempt ended short of a reply.
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
