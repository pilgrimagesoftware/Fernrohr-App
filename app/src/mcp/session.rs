//! MCP sessions (`mcp-connections` design decision 3): one per running
//! `fernrohr mcp`, so one per MCP client, open for the client's whole life.
//!
//! A [`SessionId`] is the capability a request presents (Security 4): 128 bits
//! from the OS's random source, compared in constant time, never shown in
//! `Debug` output, and valid only while its session's socket stays open. Inside
//! the app a session is an [`AgentId`] - the handle connection holds use - so
//! the secret never leaves this layer.
//!
//! A client's name is what it reported in `initialize`: a label for the user,
//! never an identity (Security 5). Nothing here or downstream decides anything
//! by it.

use crate::k8s::cluster::session::AgentId;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io;

/// The longest client name kept; a longer one is cut, so a client can't make
/// the UI carry an arbitrary blob.
const MAX_CLIENT_NAME_CHARS: usize = 64;

/// A session's secret identifier.
#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct SessionId(String);

impl SessionId {
    /// A fresh identifier: 16 bytes from the OS's random source, hex-encoded.
    pub(super) fn generate() -> io::Result<Self> {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(io::Error::other)?;
        Ok(Self(
            bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
        ))
    }

    /// Whether `other` is this identifier - never true for an empty one, which
    /// no session has.
    fn matches(&self, other: &Self) -> bool {
        !self.0.is_empty() && self == other
    }
}

/// Compares in time that depends only on the lengths, so a peer can't recover a
/// live identifier a byte at a time. The only equality an identifier has.
impl PartialEq for SessionId {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (self.0.as_bytes(), other.0.as_bytes());
        a.len() == b.len() && a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
    }
}

impl fmt::Debug for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionId([redacted])")
    }
}

/// Who a tool call comes from: the session's in-app handle and the name its
/// client reported. Carries no secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Caller {
    pub(crate) agent: AgentId,
    /// Self-reported; display only.
    pub(crate) client_name: Option<String>,
}

struct Live {
    id: SessionId,
    caller: Caller,
}

/// The sessions open now. Shared by every connection the endpoint serves.
#[derive(Default)]
pub(super) struct Sessions {
    live: Mutex<Vec<Live>>,
}

impl Sessions {
    /// Opens a session for a client that reported `client_name`: its secret
    /// identifier, to hand to the adapter, and its caller.
    pub(super) fn open(&self, client_name: Option<String>) -> io::Result<(SessionId, Caller)> {
        let id = SessionId::generate()?;
        let caller = Caller {
            agent: AgentId::next(),
            client_name: client_name.map(clean_name).filter(|name| !name.is_empty()),
        };
        self.live.lock().push(Live {
            id: id.clone(),
            caller: caller.clone(),
        });
        Ok((id, caller))
    }

    /// The caller `id` belongs to, if it names a live session. Every live
    /// session is compared, so the time taken doesn't say which one matched.
    pub(super) fn caller(&self, id: &SessionId) -> Option<Caller> {
        let live = self.live.lock();
        let mut found = None;
        for session in live.iter() {
            if session.id.matches(id) {
                found = Some(session.caller.clone());
            }
        }
        found
    }

    /// Ends the session for `agent`, if it is live: its identifier stops
    /// working at once.
    pub(super) fn close(&self, agent: AgentId) {
        self.live
            .lock()
            .retain(|session| session.caller.agent != agent);
    }

    /// Ends every live session, returning their agents so their holds can be
    /// ended too.
    pub(super) fn close_all(&self) -> Vec<AgentId> {
        self.live
            .lock()
            .drain(..)
            .map(|session| session.caller.agent)
            .collect()
    }
}

/// A reported name, cut to [`MAX_CLIENT_NAME_CHARS`] with control characters
/// removed, so it can't break a log line or the UI.
fn clean_name(name: String) -> String {
    name.chars()
        .filter(|c| !c.is_control())
        .take(MAX_CLIENT_NAME_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests;
