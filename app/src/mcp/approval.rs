//! The app-owned confirmation gate (design.md: App-owned confirmation gate):
//! no action tool changes anything until the user allows that exact action
//! in Fernrohr's own window.
//!
//! [`approve`] is the one way through. It asks one question at a time (a
//! second agent request waits for the first to be answered, rather than
//! stacking dialogs), bounds the wait, and turns the answer into `Ok` or a
//! [`ToolError`] - [`ToolError::Denied`], [`ToolError::ApprovalTimedOut`] -
//! that no write ever follows. Who asks is an [`Approver`]: the app's dialog
//! ([`UiApprover`]), or a scripted answer in tests.
//!
//! A request is withdrawn whenever its call is: a timeout, or the agent
//! hanging up, drops the waiting future, and the dialog closes with it.

use super::error::ToolError;
use super::foreground::Foreground;
use crate::config::tunnels::TunnelKind;
use crate::consts::MCP_APPROVAL_TIMEOUT;
use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, oneshot};

/// Everything the user sees before allowing an action, or a connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApprovalRequest {
    /// The dialog's title: "Scale Deployment?"
    pub(crate) title: String,
    /// The confirm button's label: "Scale".
    pub(crate) confirm: String,
    /// The tool asking, as the agent named it: `scale_workload`.
    pub(crate) tool: String,
    pub(crate) context: String,
    pub(crate) namespace: String,
    /// The target kind, qualified by group outside the core one:
    /// `Deployment (apps)`.
    pub(crate) kind: String,
    /// Every object the action touches, by name.
    pub(crate) targets: Vec<String>,
    /// The action's own values, each a label and what it changes:
    /// `("Replicas", "2 → 3")`.
    pub(crate) parameters: Vec<(String, String)>,
    /// What is being asked: an action, or a connection.
    pub(crate) asking: Asking,
}

/// What an [`ApprovalRequest`] asks the user to allow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Asking {
    /// One of the allowlisted cluster actions. `irreversible` for one that
    /// can't be taken back: the dialog's irreversible tier, where Enter
    /// cancels.
    Action { irreversible: bool },
    /// Connecting a context (`connect_context`), which lets the agent read
    /// it. It changes nothing in the cluster and disconnecting undoes it, so
    /// it is the recoverable tier (`mcp-connect-and-focus` D2).
    Connect,
}

impl ApprovalRequest {
    /// Whether the dialog is the irreversible tier.
    pub(crate) fn irreversible(&self) -> bool {
        match self.asking {
            Asking::Action { irreversible } => irreversible,
            Asking::Connect => false,
        }
    }

    /// The question `connect_context` asks before connecting `context`,
    /// naming the tunnel it is bound to and that tunnel's kind, if any.
    pub(crate) fn connect(context: &str, tunnel: Option<(&str, TunnelKind)>) -> Self {
        let tunnel = match tunnel {
            Some((name, kind)) => format!("{name} ({})", tunnel_kind_name(kind)),
            None => "None (direct connection)".to_string(),
        };
        Self {
            title: format!("Connect {context}?"),
            confirm: "Connect".into(),
            tool: "connect_context".into(),
            context: context.to_string(),
            namespace: String::new(),
            kind: String::new(),
            targets: Vec::new(),
            parameters: vec![("Tunnel".into(), tunnel)],
            asking: Asking::Connect,
        }
    }
}

/// A tunnel kind as the dialog names it.
fn tunnel_kind_name(kind: TunnelKind) -> &'static str {
    match kind {
        TunnelKind::Ssh => "SSH tunnel",
        TunnelKind::Command => "command tunnel",
        TunnelKind::Manual => "manual tunnel",
    }
}

/// The user's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::mcp) enum Answer {
    Allow,
    Deny,
}

/// Asks the user to approve a request. The returned future must withdraw the
/// question when dropped.
pub(in crate::mcp) trait Approver: Send + Sync {
    fn ask(&self, request: ApprovalRequest) -> BoxFuture<'static, Result<Answer, ToolError>>;
}

/// The gate's settings and state, shared by every call.
#[derive(Clone)]
pub(in crate::mcp) struct ApprovalGate {
    approver: Arc<dyn Approver>,
    timeout: Duration,
    /// Held while a question is open, so questions come one at a time.
    turn: Arc<Mutex<()>>,
}

impl ApprovalGate {
    /// The app's gate: its own dialog, the usual timeout.
    pub(in crate::mcp) fn ui(foreground: Foreground) -> Self {
        Self::new(Arc::new(UiApprover { foreground }), MCP_APPROVAL_TIMEOUT)
    }

    pub(in crate::mcp) fn new(approver: Arc<dyn Approver>, timeout: Duration) -> Self {
        Self {
            approver,
            timeout,
            turn: Arc::new(Mutex::new(())),
        }
    }
}

/// Asks `gate`'s approver about `request`; `Ok` only when the user allowed it.
/// The timeout covers the user's answer, not the wait for an earlier question.
pub(in crate::mcp) async fn approve(
    gate: &ApprovalGate,
    request: ApprovalRequest,
) -> Result<(), ToolError> {
    let _turn = gate.turn.lock().await;
    match tokio::time::timeout(gate.timeout, gate.approver.ask(request)).await {
        Ok(Ok(Answer::Allow)) => Ok(()),
        Ok(Ok(Answer::Deny)) => Err(ToolError::Denied),
        Ok(Err(error)) => Err(error),
        Err(_) => Err(ToolError::ApprovalTimedOut),
    }
}

/// The app's approver: `ui::agent_approval`'s dialog, in the frontmost main
/// window.
struct UiApprover {
    foreground: Foreground,
}

impl Approver for UiApprover {
    fn ask(&self, request: ApprovalRequest) -> BoxFuture<'static, Result<Answer, ToolError>> {
        let foreground = self.foreground.clone();
        async move {
            let (reply, answer) = oneshot::channel();
            let shown = foreground
                .run(move |cx| crate::ui::agent_approval::open(request, reply, cx))
                .await?;
            let Some(question) = shown else {
                return Err(ToolError::UiUnavailable);
            };
            let mut withdraw = Withdraw {
                foreground,
                question: Some(question),
            };
            // A reply dropped unanswered - its dialog closed some other way -
            // is a denial.
            let answer = answer.await.unwrap_or(false);
            withdraw.question = None;
            Ok(if answer { Answer::Allow } else { Answer::Deny })
        }
        .boxed()
    }
}

/// Closes a question's dialog if its call goes away before it is answered.
struct Withdraw {
    foreground: Foreground,
    question: Option<crate::ui::agent_approval::Question>,
}

impl Drop for Withdraw {
    fn drop(&mut self) {
        if let Some(question) = self.question.take() {
            self.foreground
                .post(move |cx| crate::ui::agent_approval::withdraw(question, cx));
        }
    }
}

#[cfg(test)]
pub(in crate::mcp) mod scripted;
