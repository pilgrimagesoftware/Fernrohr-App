//! [`ManualTransport`]: a transport whose "connect" is the user's say-so
//! (design.md D2, D3, D5).
//!
//! Connecting first tries the reachability shortcut when the tunnel has it on: a
//! TCP connect to the acquiring context's API server, which, if it answers, counts
//! as confirmed with no prompt. Otherwise it publishes a pending confirmation to
//! [`ManualConfirmations`](super::ManualConfirmations) and waits - with no timeout,
//! by request - for Proceed, which makes the forward `Up`, or Cancel, which gives up
//! for good with a reason naming the tunnel. Nothing is ever retried, and nothing
//! needs health-checking once confirmed.
//!
//! Dropping the transport (every connection waiting on it closed) withdraws its
//! confirmation, so a prompt never outlives its waiters.

use super::confirmations::{ConfirmationEvent, Decision, PendingRequest};
use crate::consts::MANUAL_TUNNEL_PROBE_TIMEOUT;
use crate::forward::supervisor::{ConnectFailure, ForwardTransport};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// The API server address the reachability shortcut tries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbeTarget {
    pub(crate) host: String,
    pub(crate) port: u16,
}

pub(crate) struct ManualTransport {
    request: PendingRequest,
    /// `Some` when the tunnel skips the prompt for a reachable API server.
    probe: Option<ProbeTarget>,
    probe_timeout: Duration,
    events: mpsc::UnboundedSender<ConfirmationEvent>,
    /// The confirmation this transport published and is waiting on.
    published: Option<u64>,
}

impl ManualTransport {
    pub(crate) fn new(
        tunnel_id: String,
        name: String,
        message: Option<String>,
        probe: Option<ProbeTarget>,
        events: mpsc::UnboundedSender<ConfirmationEvent>,
    ) -> Self {
        Self {
            request: PendingRequest {
                id: 0,
                tunnel_id,
                name,
                message,
            },
            probe,
            probe_timeout: MANUAL_TUNNEL_PROBE_TIMEOUT,
            events,
            published: None,
        }
    }

    /// The probe's timeout, shortened for a test.
    #[cfg(test)]
    pub(crate) fn with_probe_timeout(mut self, timeout: Duration) -> Self {
        self.probe_timeout = timeout;
        self
    }

    /// Asks the user, and waits for their answer.
    async fn ask(&mut self) -> Result<(), ConnectFailure> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let (resolve, answer) = oneshot::channel();
        let request = PendingRequest {
            id,
            ..self.request.clone()
        };
        let name = request.name.clone();
        if self
            .events
            .send(ConfirmationEvent::Pending { request, resolve })
            .is_err()
        {
            return Err(ConnectFailure::GiveUp(format!(
                "{name} can't be confirmed: the app is shutting down"
            )));
        }
        self.published = Some(id);
        let answer = answer.await;
        self.published = None;
        match answer {
            Ok(Decision::Proceed) => Ok(()),
            Ok(Decision::Cancel) => Err(ConnectFailure::GiveUp(format!("{name} was cancelled"))),
            // The entry went away unanswered: the app is closing it down.
            Err(_) => Err(ConnectFailure::GiveUp(format!(
                "{name} was withdrawn before it was confirmed"
            ))),
        }
    }
}

/// Whether `target` accepts a TCP connection within `timeout`.
pub(super) async fn reachable(target: &ProbeTarget, timeout: Duration) -> bool {
    let connect = tokio::net::TcpStream::connect((target.host.as_str(), target.port));
    matches!(tokio::time::timeout(timeout, connect).await, Ok(Ok(_)))
}

impl ForwardTransport for ManualTransport {
    async fn connect(&mut self) -> Result<(), String> {
        self.connect_outcome()
            .await
            .map_err(|failure| match failure {
                ConnectFailure::Retry(reason) | ConnectFailure::GiveUp(reason) => reason,
            })
    }

    /// Nothing runs, so nothing can stop running: a dropped VPN shows up as the
    /// connections' own interruptions (design.md Non-Goals).
    async fn health_check(&mut self) -> Result<(), String> {
        Ok(())
    }

    async fn connect_outcome(&mut self) -> Result<(), ConnectFailure> {
        if let Some(target) = &self.probe
            && reachable(target, self.probe_timeout).await
        {
            let _ = self.events.send(ConfirmationEvent::Settled {
                tunnel_id: self.request.tunnel_id.clone(),
            });
            return Ok(());
        }
        self.ask().await
    }
}

impl Drop for ManualTransport {
    fn drop(&mut self) {
        if let Some(id) = self.published.take() {
            let _ = self.events.send(ConfirmationEvent::Withdrawn { id });
        }
    }
}

#[cfg(test)]
mod tests;
