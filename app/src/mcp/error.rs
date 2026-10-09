//! [`ToolError`]: every failure an MCP client can be told about, built so that
//! nothing reaches it but what the variant itself names (`agent-mcp`:
//! Credential and error safety). Upstream errors are mapped into it, never
//! passed through: a Kubernetes API error keeps only its status code, reason
//! and a redacted message, and every other client failure - TLS, auth plugin,
//! proxy, I/O, any of which can carry a credential or a kubeconfig path - keeps
//! nothing but which context it was.

use super::redact::redact;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fmt;

/// A tool call's failure, as the app reports it to the adapter and the adapter
/// to the MCP client. Its serialized form is the structured error a client
/// sees, tagged by [`Self::code`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub(crate) enum ToolError {
    /// No Fernrohr app answered the adapter, or it refused the handshake.
    Unavailable { reason: Unavailability },
    /// No tool is registered under this name.
    UnknownTool { name: String },
    /// The arguments don't match the tool's input.
    InvalidArguments { message: String },
    /// No context of this name is known to the app.
    UnknownContext { context: String },
    /// The context is known but not connected.
    Disconnected { context: String },
    /// The context's discovery data has no such resource kind.
    UnsupportedKind { kind: String },
    /// More than one API group in the context has a kind of this name; the
    /// client has to say which.
    AmbiguousKind { kind: String, groups: Vec<String> },
    /// The kind exists, but its discovery data doesn't offer this operation
    /// (`list`, say, for a kind that can only be read one at a time).
    UnsupportedOperation { kind: String, operation: String },
    /// The Kubernetes API refused the request.
    Kubernetes {
        status: u16,
        reason: String,
        message: String,
    },
    /// The request never got an API answer; why is withheld, since the
    /// client-side failure can carry credentials.
    ConnectionFailed { context: String },
    /// The tool's result is larger than the endpoint sends.
    ResultTooLarge { limit: usize },
    /// The app has no user interface to serve the request on (quitting, or no
    /// window open).
    UiUnavailable,
    /// The app failed in a way the client can do nothing about; the detail
    /// stays in the app's log.
    Internal,
}

/// Why the adapter couldn't reach a usable app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Unavailability {
    /// No app is running, or its endpoint isn't listening.
    NotRunning,
    /// The app refused this adapter's token.
    Unauthorized,
    /// The app speaks a different internal protocol version: the adapter and
    /// the app are different builds.
    Incompatible,
}

impl ToolError {
    /// The stable machine-readable code: the serialized tag, and what a log
    /// line records in place of the error's fields.
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Unavailable { .. } => "unavailable",
            Self::UnknownTool { .. } => "unknown_tool",
            Self::InvalidArguments { .. } => "invalid_arguments",
            Self::UnknownContext { .. } => "unknown_context",
            Self::Disconnected { .. } => "disconnected",
            Self::UnsupportedKind { .. } => "unsupported_kind",
            Self::AmbiguousKind { .. } => "ambiguous_kind",
            Self::UnsupportedOperation { .. } => "unsupported_operation",
            Self::Kubernetes { .. } => "kubernetes",
            Self::ConnectionFailed { .. } => "connection_failed",
            Self::ResultTooLarge { .. } => "result_too_large",
            Self::UiUnavailable => "ui_unavailable",
            Self::Internal => "internal",
        }
    }

    /// Arguments that failed to deserialize into a tool's input. serde's
    /// message names the field and echoes at most the caller's own value; it
    /// is still scrubbed, like everything else a client is shown.
    pub(crate) fn invalid_arguments(error: &serde_json::Error) -> Self {
        Self::InvalidArguments {
            message: redact(&error.to_string()),
        }
    }

    /// A `kube` failure for a request against `context`, mapped as the module
    /// docs describe.
    pub(crate) fn from_kube(context: &str, error: &kube::Error) -> Self {
        match error {
            // kube's stand-in for a body that wasn't a `Status`: the "message"
            // is whatever the server, or a proxy in front of it, sent back.
            kube::Error::Api(status) if status.reason == UNPARSED_ERROR_REASON => {
                Self::Kubernetes {
                    status: status.code,
                    reason: "Unknown".to_string(),
                    message: String::new(),
                }
            }
            kube::Error::Api(status) => Self::Kubernetes {
                status: status.code,
                reason: status.reason.clone(),
                message: redact(&status.message),
            },
            _ => Self::ConnectionFailed {
                context: context.to_string(),
            },
        }
    }

    /// The structured form an MCP client receives: the serialized fields plus
    /// a readable `message`.
    pub(crate) fn to_json(&self) -> Value {
        let mut object = match serde_json::to_value(self) {
            Ok(Value::Object(object)) => object,
            // A tagged enum of plain fields always serializes to an object;
            // keep the code if it somehow didn't.
            _ => Map::from_iter([("code".to_string(), Value::from(self.code()))]),
        };
        object
            .entry("message")
            .or_insert_with(|| Value::from(self.to_string()));
        Value::Object(object)
    }
}

/// kube's reason for an error body it couldn't parse as a `Status`
/// (`kube_client::client::handle_api_errors`); `k8s::error` matches it too.
const UNPARSED_ERROR_REASON: &str = "Failed to parse error data";

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable {
                reason: Unavailability::NotRunning,
            } => f.write_str("Fernrohr is not running; start the app and try again"),
            Self::Unavailable {
                reason: Unavailability::Unauthorized,
            } => f.write_str("the running Fernrohr app refused this connection"),
            Self::Unavailable {
                reason: Unavailability::Incompatible,
            } => f.write_str("this fernrohr command and the running app are different versions"),
            Self::UnknownTool { name } => write!(f, "no tool named {name:?}"),
            Self::InvalidArguments { message } => write!(f, "invalid arguments: {message}"),
            Self::UnknownContext { context } => write!(f, "no context named {context:?}"),
            Self::Disconnected { context } => write!(f, "context {context:?} is not connected"),
            Self::UnsupportedKind { kind } => {
                write!(f, "resource kind {kind:?} is not available in this context")
            }
            Self::AmbiguousKind { kind, groups } => {
                let groups: Vec<_> = groups.iter().map(|group| group_label(group)).collect();
                write!(
                    f,
                    "{kind:?} names a kind in several API groups ({}); pass `group`",
                    groups.join(", ")
                )
            }
            Self::UnsupportedOperation { kind, operation } => {
                write!(f, "{kind} does not support {operation}")
            }
            Self::Kubernetes {
                status,
                reason,
                message,
            } if message.is_empty() => write!(f, "Kubernetes API error {status} ({reason})"),
            Self::Kubernetes {
                status,
                reason,
                message,
            } => write!(f, "Kubernetes API error {status} ({reason}): {message}"),
            Self::ConnectionFailed { context } => {
                write!(f, "could not reach the cluster for context {context:?}")
            }
            Self::ResultTooLarge { limit } => {
                write!(f, "the result is larger than the {limit}-byte limit")
            }
            Self::UiUnavailable => f.write_str("Fernrohr has no window to show this in"),
            Self::Internal => f.write_str("Fernrohr failed to handle the request"),
        }
    }
}

/// How a group reads in a message: the core group's name is empty.
fn group_label(group: &str) -> &str {
    if group.is_empty() { "core" } else { group }
}

#[cfg(test)]
mod tests;
