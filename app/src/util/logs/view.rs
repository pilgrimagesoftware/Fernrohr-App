//! The pure view-model for a pod's log panel: the events a log stream feeds
//! it ([`LogEvent`]), its follow/pause state ([`FollowState`]), and the line
//! history, container selection, and terminal-state handling ([`LogsView`]).
//! GPUI-free so it's directly unit-testable.

/// One event off a pod container's log stream. Kept separate from the
/// resource-index coalescing path (runtime::drain_coalescing) - logs are
/// ordered text, not keyed state, so every line must render, in order, with
/// nothing folded away.
pub enum LogEvent {
    Line(String),
    /// The stream ended normally (e.g. the pod was deleted).
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
}

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
    lines: Vec<String>,
    follow: FollowState,
    containers: Vec<String>,
    #[allow(dead_code)]
    selected_container: String,
    terminal_message: Option<String>,
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
            terminal_detail: None,
        }
    }

    pub fn lines(&self) -> &[String] {
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
        self.lines.push(line);
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
        self.terminal_detail = None;
        true
    }

    pub fn apply(&mut self, event: LogEvent) {
        match event {
            LogEvent::Line(line) => self.append_line(line),
            LogEvent::Ended => {
                self.terminal_message = Some("Log stream ended.".into());
                self.terminal_detail = None;
            }
            LogEvent::RequestFailed { message, detail } => {
                self.terminal_message = Some(format!("Couldn't start log stream: {message}"));
                self.terminal_detail = Some(detail);
            }
        }
    }
}

#[cfg(test)]
mod tests;
