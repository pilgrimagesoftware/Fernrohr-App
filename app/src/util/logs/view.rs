//! The pure view-model for a pod's log panel: the events a log stream feeds
//! it ([`LogEvent`]), its follow/pause state ([`FollowState`]), and the line
//! history, container selection, and terminal-state handling ([`LogsView`]).
//! GPUI-free so it's directly unit-testable.

use super::line::{LineSource, LogLine};

/// One event off a pod container's log stream. Kept separate from the
/// resource-index coalescing path (runtime::drain_coalescing) - logs are
/// ordered text, not keyed state, so every line must render, in order, with
/// nothing folded away.
pub enum LogEvent {
    Line(String),
    /// The stream ended normally: the pod was deleted, its container exited,
    /// or - for a container that had already terminated - its whole log was
    /// read.
    Ended,
    /// The stream never started (e.g. the container hasn't started yet), or
    /// broke while reading it. `message` is what a person reads - readable
    /// prose, never a client library's `Debug` dump; `detail` is that same
    /// failure's full technical rendering, kept alongside rather than
    /// discarded, for a report that needs more than the summary.
    RequestFailed {
        message: String,
        detail: String,
    },
    /// Previous logs were asked for, and the container has no previous instance -
    /// it hasn't restarted (`k9s-remaining-keybindings` 5.2).
    NoPreviousInstance,
}

/// What the panel says when previous logs were asked for and there are none.
pub const NO_PREVIOUS_INSTANCE: &str =
    "This container hasn't restarted, so there is no previous instance to show logs for.";

/// The marker after the last line of a stream that ended (`completed-pod-logs`),
/// and the line a tagged stream writes when one of its containers ends.
pub const ENDED_MARKER: &str = "\u{2014} log stream ended";

/// What an ended stream with no lines says in place of the marker, so an empty
/// log reads differently from a failure.
pub const NO_OUTPUT: &str = "The container wrote no log output.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowState {
    Following,
    /// The user scrolled up to read history; new lines still arrive but the
    /// view doesn't jump to them until they scroll back to the bottom.
    Paused,
}

/// View-model for one pod's log panel: line history, follow state, and
/// container selection. GPUI-free so it's directly unit-testable.
pub struct LogsView {
    lines: Vec<LogLine>,
    follow: FollowState,
    containers: Vec<String>,
    #[allow(dead_code)]
    selected_container: String,
    /// What took the panel over instead of its lines: a failure, or no
    /// previous instance. A plain end is [`Self::ended`], not this.
    terminal_message: Option<String>,
    /// Whether the stream ended normally. Its lines stay, with
    /// [`ENDED_MARKER`] after them (`completed-pod-logs`).
    ended: bool,
    /// The failure's full technical detail, alongside `terminal_message` - set
    /// only for [`LogEvent::RequestFailed`], never for the plain "stream
    /// ended" state, which has no error behind it to detail.
    terminal_detail: Option<String>,
}

impl LogsView {
    /// `containers` must be non-empty; the first one is selected by default.
    pub fn new(containers: Vec<String>) -> Self {
        let selected_container = containers.first().cloned().unwrap_or_default();
        Self {
            lines: Vec::new(),
            follow: FollowState::Following,
            containers,
            selected_container,
            terminal_message: None,
            ended: false,
            terminal_detail: None,
        }
    }

    pub fn lines(&self) -> &[LogLine] {
        &self.lines
    }

    pub fn follow_state(&self) -> FollowState {
        self.follow
    }

    pub fn selected_container(&self) -> &str {
        &self.selected_container
    }

    pub fn containers(&self) -> &[String] {
        &self.containers
    }

    pub fn terminal_message(&self) -> Option<&str> {
        self.terminal_message.as_deref()
    }

    /// Whether the stream ended normally, its lines kept.
    pub fn ended(&self) -> bool {
        self.ended
    }

    /// The row after the last line of an ended stream: [`ENDED_MARKER`], or
    /// [`NO_OUTPUT`] when it never wrote a line. `None` while it streams.
    pub fn end_marker(&self) -> Option<&'static str> {
        match (self.ended, self.lines.is_empty()) {
            (false, _) => None,
            (true, true) => Some(NO_OUTPUT),
            (true, false) => Some(ENDED_MARKER),
        }
    }

    /// The rows the list draws: every line, and the end marker if there is
    /// one - what scrolling to the bottom has to reach.
    pub fn row_count(&self) -> usize {
        self.lines.len() + usize::from(self.end_marker().is_some())
    }

    /// The full technical detail behind [`Self::terminal_message`], when the
    /// terminal state is a failure rather than a plain "stream ended."
    pub fn terminal_detail(&self) -> Option<&str> {
        self.terminal_detail.as_deref()
    }

    /// Whether a container picker needs to be shown at all - a single
    /// container needs no pick.
    pub fn needs_container_picker(&self) -> bool {
        self.containers.len() > 1
    }

    pub fn append_line(&mut self, line: String) {
        self.lines.push(LogLine::plain(line));
    }

    pub fn scroll_up(&mut self) {
        self.follow = FollowState::Paused;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.follow = FollowState::Following;
    }

    /// Selects `container`. Returns whether the selection actually changed -
    /// the caller restarts the stream only then, and this clears the history
    /// and terminal state from the previous container's stream. Unused now
    /// that `LogsPanel::switch_container` rebuilds a fresh `LogsView` instead
    /// (matching `sync`'s own pattern) - kept for the unit tests exercising
    /// the view model directly, and as the natural API if that changes.
    #[allow(dead_code)]
    pub fn select_container(&mut self, container: &str) -> bool {
        if container == self.selected_container {
            return false;
        }
        self.selected_container = container.to_string();
        self.lines.clear();
        self.terminal_message = None;
        self.ended = false;
        self.terminal_detail = None;
        true
    }

    /// Applies `event` from one of several containers streaming into this view
    /// (#150): its line tagged with `source`, and its end or failure a line of
    /// its own rather than the view's terminal state - the other containers
    /// keep streaming.
    pub fn apply_from(&mut self, source: &LineSource, event: LogEvent) {
        let line = match event {
            LogEvent::Line(line) => line,
            LogEvent::Ended => ENDED_MARKER.to_string(),
            LogEvent::RequestFailed { message, .. } => {
                format!("\u{2014} couldn't stream logs: {message}")
            }
            // Only a previous-instance request gets this, and a tagged stream
            // never makes one.
            LogEvent::NoPreviousInstance => return,
        };
        self.lines.push(LogLine::tagged(source.clone(), line));
    }

    pub fn apply(&mut self, event: LogEvent) {
        match event {
            LogEvent::Line(line) => self.append_line(line),
            // The lines stay, with the marker after them, rather than giving
            // the panel over to a message (`completed-pod-logs`).
            LogEvent::Ended => self.ended = true,
            LogEvent::RequestFailed { message, detail } => {
                self.terminal_message = Some(format!("Couldn't start log stream: {message}"));
                self.terminal_detail = Some(detail);
            }
            LogEvent::NoPreviousInstance => {
                self.terminal_message = Some(NO_PREVIOUS_INSTANCE.into());
                self.terminal_detail = None;
            }
        }
    }
}

#[cfg(test)]
mod tests;
