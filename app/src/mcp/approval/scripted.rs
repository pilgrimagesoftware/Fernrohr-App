//! An [`Approver`] that answers from a script, for tests of the tools behind
//! the gate: it records every request it was shown, and how many it saw
//! withdrawn before answering.

use super::{Answer, ApprovalGate, ApprovalRequest, Approver};
use crate::mcp::error::ToolError;
use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// How the scripted user answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::mcp) enum Script {
    Allow,
    Deny,
    /// Never answers: the question stays open until withdrawn.
    Never,
}

pub(in crate::mcp) struct Scripted {
    script: Script,
    asked: Mutex<Vec<ApprovalRequest>>,
    withdrawn: Arc<AtomicUsize>,
}

impl Scripted {
    pub(in crate::mcp) fn new(script: Script) -> Arc<Self> {
        Arc::new(Self {
            script,
            asked: Mutex::default(),
            withdrawn: Arc::default(),
        })
    }

    pub(in crate::mcp) fn deny() -> Arc<Self> {
        Self::new(Script::Deny)
    }

    /// A gate asking this approver, with a timeout no test reaches by
    /// accident.
    pub(in crate::mcp) fn gate(self: &Arc<Self>) -> ApprovalGate {
        self.gate_with_timeout(Duration::from_secs(30))
    }

    pub(in crate::mcp) fn gate_with_timeout(self: &Arc<Self>, timeout: Duration) -> ApprovalGate {
        ApprovalGate::new(self.clone(), timeout)
    }

    /// Every request shown so far.
    pub(in crate::mcp) fn asked(&self) -> Vec<ApprovalRequest> {
        self.asked.lock().clone()
    }

    /// How many questions were withdrawn unanswered.
    pub(in crate::mcp) fn withdrawn(&self) -> usize {
        self.withdrawn.load(Ordering::SeqCst)
    }
}

impl Approver for Scripted {
    fn ask(&self, request: ApprovalRequest) -> BoxFuture<'static, Result<Answer, ToolError>> {
        self.asked.lock().push(request);
        match self.script {
            Script::Allow => std::future::ready(Ok(Answer::Allow)).boxed(),
            Script::Deny => std::future::ready(Ok(Answer::Deny)).boxed(),
            Script::Never => {
                let withdrawn = Counted(self.withdrawn.clone());
                async move {
                    let _withdrawn = withdrawn;
                    std::future::pending().await
                }
                .boxed()
            }
        }
    }
}

/// Counts one withdrawal when the never-answered question's future drops.
struct Counted(Arc<AtomicUsize>);

impl Drop for Counted {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
