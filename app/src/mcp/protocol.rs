//! The internal RPC between `fernrohr mcp` and the app: its messages, and the
//! token the handshake carries.
//!
//! One connection serves one request:
//!
//! 1. The adapter sends a [`Hello`]; the app answers a [`HelloReply`].
//! 2. If accepted, the adapter sends one [`Request`]; the app answers one
//!    [`Reply`] and closes.
//!
//! The adapter closing its end before the reply cancels the request - the
//! one signal a call awaiting the user's approval needs (design.md Risks:
//! client disconnects). Each message is one `frame`.
//!
//! [`PROTOCOL_VERSION`] changes whenever these shapes do. The adapter and the
//! app are one binary, so a mismatch means two installed builds, which the
//! handshake reports rather than misreading the other side's messages.

use super::tools::{ToolResult, ToolSpec};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fmt;
use std::io;
use std::path::Path;

/// The version of the shapes in this module.
pub(super) const PROTOCOL_VERSION: u32 = 1;

/// The adapter's opening message.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Hello {
    pub(super) protocol: u32,
    pub(super) token: EndpointToken,
}

/// The app's answer to a [`Hello`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub(super) enum HelloReply {
    Accepted,
    Rejected { reason: Rejection },
}

/// Why the app refused a [`Hello`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Rejection {
    /// The token isn't this launch's: missing, wrong, or a previous run's.
    Token,
    /// The adapter speaks a different [`PROTOCOL_VERSION`].
    Protocol,
}

/// What the adapter asks for, once accepted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Request {
    ListTools,
    CallTool {
        name: String,
        #[serde(default)]
        arguments: Map<String, Value>,
    },
}

/// The app's answer to a [`Request`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "snake_case")]
pub(super) enum Reply {
    Tools { tools: Vec<ToolSpec> },
    Called { result: ToolResult },
}

/// The per-launch secret an adapter proves itself with. Its `Debug` never
/// shows it, so it can't reach a log line by accident.
#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub(super) struct EndpointToken(String);

impl EndpointToken {
    /// A fresh token: 32 bytes from the OS's random source, hex-encoded.
    pub(super) fn generate() -> io::Result<Self> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(io::Error::other)?;
        Ok(Self(
            bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
        ))
    }

    /// The token stored at `path`.
    pub(super) fn read(path: &Path) -> io::Result<Self> {
        Ok(Self(std::fs::read_to_string(path)?.trim().to_string()))
    }

    /// The token's text, for writing it to its file.
    pub(super) fn expose(&self) -> &str {
        &self.0
    }

    /// Compares two tokens in time that depends only on their lengths, so a
    /// peer can't recover the token a byte at a time by timing rejections.
    pub(super) fn matches(&self, other: &Self) -> bool {
        let (a, b) = (self.0.as_bytes(), other.0.as_bytes());
        a.len() == b.len()
            && !a.is_empty()
            && a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
    }
}

impl fmt::Debug for EndpointToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EndpointToken([redacted])")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::error::ToolError;
    use crate::mcp::tools::{ToolKind, ToolOutput};
    use serde_json::json;

    #[test]
    fn tokens_are_fresh_each_time_and_match_only_themselves() {
        let a = EndpointToken::generate().unwrap();
        let b = EndpointToken::generate().unwrap();
        assert_eq!(a.expose().len(), 64);
        assert!(a.matches(&a.clone()));
        assert!(!a.matches(&b));
        assert!(!a.matches(&EndpointToken(String::new())));
        assert!(!EndpointToken(String::new()).matches(&EndpointToken(String::new())));
    }

    #[test]
    fn a_token_never_shows_in_debug_output() {
        let token = EndpointToken::generate().unwrap();
        let hello = Hello {
            protocol: PROTOCOL_VERSION,
            token: token.clone(),
        };
        assert!(!format!("{hello:?}").contains(token.expose()));
    }

    #[test]
    fn a_hello_without_a_token_does_not_parse() {
        assert!(serde_json::from_value::<Hello>(json!({"protocol": 1})).is_err());
        assert!(
            serde_json::from_value::<Hello>(json!({"protocol": 1, "token": "t", "x": 1})).is_err()
        );
    }

    #[test]
    fn messages_have_stable_wire_shapes() {
        assert_eq!(
            serde_json::to_value(HelloReply::Rejected {
                reason: Rejection::Token
            })
            .unwrap(),
            json!({"result": "rejected", "reason": "token"})
        );
        assert_eq!(
            serde_json::to_value(Request::CallTool {
                name: "list_contexts".into(),
                arguments: Map::new(),
            })
            .unwrap(),
            json!({"method": "call_tool", "name": "list_contexts", "arguments": {}})
        );
        assert_eq!(
            serde_json::from_value::<Request>(json!({"method": "call_tool", "name": "x"})).unwrap(),
            Request::CallTool {
                name: "x".into(),
                arguments: Map::new()
            }
        );
    }

    #[test]
    fn replies_round_trip() {
        let replies = [
            Reply::Tools {
                tools: vec![ToolSpec {
                    name: "echo".into(),
                    title: "Echo".into(),
                    description: "d".into(),
                    kind: ToolKind::Read,
                    input_schema: Map::new(),
                }],
            },
            Reply::Called {
                result: Ok(ToolOutput {
                    content: Map::from_iter([("a".to_string(), json!(1))]),
                    truncated: true,
                }),
            },
            Reply::Called {
                result: Err(ToolError::Internal),
            },
        ];
        for reply in replies {
            let wire = serde_json::to_string(&reply).unwrap();
            assert_eq!(serde_json::from_str::<Reply>(&wire).unwrap(), reply);
        }
    }
}
