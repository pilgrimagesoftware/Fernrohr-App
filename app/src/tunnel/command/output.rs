//! The last lines a command tunnel's command printed, on stdout or stderr - what a
//! failure reason shows, since a vendor CLI says what went wrong (a missing login, a
//! host-key prompt) there.

use crate::consts::COMMAND_TUNNEL_OUTPUT_LINES;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt as _, AsyncRead, BufReader};

/// A bounded ring of output lines, shared between the readers draining the
/// command's pipes and the transport that reports them.
#[derive(Clone, Default)]
pub(super) struct OutputTail(Arc<Mutex<VecDeque<String>>>);

impl OutputTail {
    pub(super) fn push(&self, line: String) {
        let mut lines = self.0.lock();
        if lines.len() == COMMAND_TUNNEL_OUTPUT_LINES {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    pub(super) fn clear(&self) {
        self.0.lock().clear();
    }

    /// The lines, oldest first, joined; empty when the command printed nothing.
    pub(super) fn text(&self) -> String {
        let lines = self.0.lock();
        lines
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// `reason`, followed by the recent output when there is any.
    pub(super) fn with_reason(&self, reason: impl std::fmt::Display) -> String {
        let text = self.text();
        if text.trim().is_empty() {
            reason.to_string()
        } else {
            format!("{reason}\n\nRecent output:\n{text}")
        }
    }

    /// Reads `pipe` line by line into the tail until it closes.
    pub(super) async fn drain(self, pipe: impl AsyncRead + Unpin) {
        let mut lines = BufReader::new(pipe).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            self.push(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_the_most_recent_lines() {
        let tail = OutputTail::default();
        for n in 0..COMMAND_TUNNEL_OUTPUT_LINES + 5 {
            tail.push(format!("line {n}"));
        }
        let text = tail.text();
        assert!(!text.contains("line 4\n"), "the oldest lines dropped");
        assert!(text.starts_with("line 5\n"));
        assert!(text.ends_with(&format!("line {}", COMMAND_TUNNEL_OUTPUT_LINES + 4)));
    }

    #[test]
    fn a_reason_carries_the_output_only_when_there_is_some() {
        let tail = OutputTail::default();
        assert_eq!(tail.with_reason("exited"), "exited");
        tail.push("ERROR: please log in".into());
        assert_eq!(
            tail.with_reason("exited"),
            "exited\n\nRecent output:\nERROR: please log in"
        );
    }
}
