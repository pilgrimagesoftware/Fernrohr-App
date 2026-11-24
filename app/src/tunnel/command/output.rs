//! The last lines a command tunnel's command printed, on stdout or stderr - what a
//! failure reason shows, since a vendor CLI says what went wrong (a missing login, a
//! host-key prompt) there.
//!
//! The pipes are read in chunks, not line by line, so output that doesn't end in a
//! newline is kept too: a prompt (`Enter passphrase for key '...': `) usually
//! doesn't, and it's the one thing that explains a command stuck waiting (#423).
//! Each pipe keeps its own unfinished line, so stdout and stderr don't splice.

use super::prompt::looks_like_prompt;
use crate::consts::COMMAND_TUNNEL_OUTPUT_LINES;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt as _};

/// Which of the command's pipes output came from.
#[derive(Clone, Copy, Debug)]
pub(super) enum Pipe {
    Stdout = 0,
    Stderr = 1,
}

/// The longest unfinished line kept before it's taken as a line anyway, so output
/// that never ends a line (a progress bar redrawn with `\r`) stays bounded.
const PARTIAL_LIMIT: usize = 1024;

#[derive(Default)]
struct Lines {
    /// Finished lines, oldest first, at most [`COMMAND_TUNNEL_OUTPUT_LINES`].
    done: VecDeque<String>,
    /// Each pipe's unfinished line, as raw bytes so a UTF-8 character split
    /// across reads still decodes once whole.
    partial: [Vec<u8>; 2],
    /// When the command last printed anything.
    last_output: Option<Instant>,
}

impl Lines {
    fn push(&mut self, line: String) {
        if self.done.len() == COMMAND_TUNNEL_OUTPUT_LINES {
            self.done.pop_front();
        }
        self.done.push_back(line);
    }

    /// Ends `pipe`'s unfinished line, if it has one, as a finished line.
    fn finish(&mut self, pipe: Pipe) {
        let bytes = std::mem::take(&mut self.partial[pipe as usize]);
        if !bytes.is_empty() {
            self.push(decode(&bytes));
        }
    }

    fn unfinished(&self) -> impl Iterator<Item = String> + '_ {
        self.partial
            .iter()
            .filter(|bytes| !bytes.is_empty())
            .map(|bytes| decode(bytes))
    }
}

fn decode(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_end_matches('\r')
        .to_string()
}

/// A bounded ring of output lines, shared between the readers draining the
/// command's pipes and the transport that reports them.
#[derive(Clone, Default)]
pub(super) struct OutputTail(Arc<Mutex<Lines>>);

impl OutputTail {
    #[cfg(test)]
    pub(super) fn push(&self, line: String) {
        let mut lines = self.0.lock();
        lines.push(line);
        lines.last_output = Some(Instant::now());
    }

    pub(super) fn clear(&self) {
        *self.0.lock() = Lines::default();
    }

    /// Takes in `bytes` read from `pipe`: each newline ends a line, and what
    /// follows the last one stays unfinished until more arrives.
    fn take(&self, pipe: Pipe, bytes: &[u8]) {
        let mut lines = self.0.lock();
        lines.last_output = Some(Instant::now());
        for &byte in bytes {
            if byte == b'\n' {
                lines.finish(pipe);
                continue;
            }
            lines.partial[pipe as usize].push(byte);
            if lines.partial[pipe as usize].len() >= PARTIAL_LIMIT {
                lines.finish(pipe);
            }
        }
    }

    /// The lines, oldest first, then any unfinished ones, joined; empty when the
    /// command printed nothing.
    pub(super) fn text(&self) -> String {
        let lines = self.0.lock();
        lines
            .done
            .iter()
            .cloned()
            .chain(lines.unfinished())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The prompt the command is waiting at, if the last thing it printed looks
    /// like one ([`looks_like_prompt`]) and it has printed nothing since: for
    /// `quiet`, when that's an unfinished line, as prompts usually are; for the
    /// longer `line_quiet`, when it's the last finished line.
    pub(super) fn waiting_prompt(&self, quiet: Duration, line_quiet: Duration) -> Option<String> {
        let lines = self.0.lock();
        let silent_for = lines.last_output?.elapsed();
        let unfinished: Vec<String> = lines.unfinished().collect();
        let (last, needed) = if unfinished.is_empty() {
            (lines.done.back().cloned().into_iter().collect(), line_quiet)
        } else {
            (unfinished, quiet)
        };
        if silent_for < needed {
            return None;
        }
        last.into_iter()
            .find(|text| looks_like_prompt(text))
            .map(|text| text.trim().to_string())
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

    /// Reads `pipe` into the tail until it closes; an unfinished last line is
    /// kept as a line.
    pub(super) async fn drain(self, from: Pipe, mut pipe: impl AsyncRead + Unpin) {
        let mut buffer = [0u8; 4096];
        while let Ok(read @ 1..) = pipe.read(&mut buffer).await {
            self.take(from, &buffer[..read]);
        }
        self.0.lock().finish(from);
    }
}

#[cfg(test)]
mod tests;
