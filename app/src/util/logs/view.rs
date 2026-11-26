//! The pure view-model for a pod's log panel: the events a log stream feeds
//! it ([`LogEvent`]), its follow/pause state ([`FollowState`]), and the line
//! history, container selection, and terminal-state handling ([`LogsView`]).
//! GPUI-free so it's directly unit-testable.

use super::ansi::{self, Style};
use super::line::{LineSource, LogLine};
use std::collections::HashMap;
use std::sync::Arc;

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

/// Where a single stream stands. A failure or a missing previous instance takes the
/// panel over; a plain end keeps the lines, with [`ENDED_MARKER`] after them
/// (`completed-pod-logs`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum StreamState {
    #[default]
    Live,
    Ended,
    /// What took the panel over instead of its lines, and - for a failed request
    /// only - its full technical detail.
    TakenOver {
        message: String,
        detail: Option<String>,
    },
}

/// View-model for one pod's log panel: line history, follow state, and
/// container selection. GPUI-free so it's directly unit-testable.
pub struct LogsView {
    lines: Vec<LogLine>,
    follow: FollowState,
    containers: Vec<String>,
    /// Which of `containers` are sidecars, which the picker marks (#398).
    sidecars: Vec<String>,
    selected_container: String,
    state: StreamState,
    /// The ANSI style a single stream's last line ended in, which its next
    /// starts in (#309).
    carried: Style,
    /// The same for each of several streams, by tag: one pod's colour never
    /// runs into another's lines.
    carried_by_tag: HashMap<Arc<str>, Style>,
    /// The widest line so far, as `(index, width)`: kept as lines arrive,
    /// so the panel sizes its list without rescanning them every frame (#365).
    widest: Option<(usize, usize)>,
}

impl LogsView {
    /// `containers` must be non-empty; the first one is selected by default.
    pub fn new(containers: Vec<String>) -> Self {
        let selected_container = containers.first().cloned().unwrap_or_default();
        Self {
            lines: Vec::new(),
            follow: FollowState::Following,
            containers,
            sidecars: Vec::new(),
            selected_container,
            state: StreamState::Live,
            carried: Style::default(),
            carried_by_tag: HashMap::new(),
            widest: None,
        }
    }

    pub fn lines(&self) -> &[LogLine] {
        &self.lines
    }

    /// The widest line as `(index, width)` in [`LogLine::width`]'s
    /// characters, or `None` with no lines. The first of equals.
    pub fn widest_line(&self) -> Option<(usize, usize)> {
        self.widest
    }

    /// Appends `line`, keeping [`Self::widest_line`] up to date - every line
    /// goes in through here.
    fn push(&mut self, line: LogLine) {
        let width = line.width();
        if self.widest.is_none_or(|(_, widest)| width > widest) {
            self.widest = Some((self.lines.len(), width));
        }
        self.lines.push(line);
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

    /// Marks `sidecars`, some of [`Self::containers`], as sidecars.
    pub fn with_sidecars(mut self, sidecars: Vec<String>) -> Self {
        self.sidecars = sidecars;
        self
    }

    pub fn sidecars(&self) -> &[String] {
        &self.sidecars
    }

    /// How the container picker names `container`: marked when it is a
    /// sidecar.
    pub fn container_label(&self, container: &str) -> String {
        let sidecar = self.sidecars.iter().any(|name| name == container);
        crate::k8s::resource::pods::container_label(container, sidecar)
    }

    /// What took the panel over instead of its lines, if anything did.
    pub fn terminal_message(&self) -> Option<&str> {
        match &self.state {
            StreamState::TakenOver { message, .. } => Some(message),
            StreamState::Live | StreamState::Ended => None,
        }
    }

    /// Whether the stream ended normally, its lines kept.
    pub fn ended(&self) -> bool {
        self.state == StreamState::Ended
    }

    /// The row after the last line of an ended stream: [`ENDED_MARKER`], or
    /// [`NO_OUTPUT`] when it never wrote a line. `None` while it streams.
    pub fn end_marker(&self) -> Option<&'static str> {
        match (self.ended(), self.lines.is_empty()) {
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
        match &self.state {
            StreamState::TakenOver { detail, .. } => detail.as_deref(),
            StreamState::Live | StreamState::Ended => None,
        }
    }

    /// Whether a container picker needs to be shown at all - a single
    /// container needs no pick.
    pub fn needs_container_picker(&self) -> bool {
        self.containers.len() > 1
    }

    /// Appends a single stream's line, its escapes parsed into styling once,
    /// here, in the style the stream's last line ended in.
    pub fn append_line(&mut self, line: String) {
        let parsed = ansi::parse(line, self.carried);
        self.carried = parsed.end;
        self.push(LogLine::styled(None, parsed.text, parsed.spans));
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
        self.widest = None;
        self.state = StreamState::Live;
        true
    }

    /// Applies `event` from one of several containers streaming into this view
    /// (#150): its line tagged with `source`, and its end or failure a line of
    /// its own rather than the view's terminal state - the other containers
    /// keep streaming.
    pub fn apply_from(&mut self, source: &LineSource, event: LogEvent) {
        // The stream's own lines carry its styling from one to the next; the
        // panel's lines about it are plain, and end that styling.
        let line = match event {
            LogEvent::Line(line) => {
                let carried = self
                    .carried_by_tag
                    .get(source.tag())
                    .copied()
                    .unwrap_or_default();
                let parsed = ansi::parse(line, carried);
                self.carried_by_tag.insert(source.tag().into(), parsed.end);
                LogLine::styled(Some(source.clone()), parsed.text, parsed.spans)
            }
            LogEvent::Ended => {
                self.carried_by_tag.remove(source.tag());
                LogLine::notice(source.clone(), ENDED_MARKER.to_string())
            }
            LogEvent::RequestFailed { message, .. } => {
                self.carried_by_tag.remove(source.tag());
                LogLine::notice(
                    source.clone(),
                    format!("\u{2014} couldn't stream logs: {message}"),
                )
            }
            // Only a previous-instance request gets this, and a tagged stream
            // never makes one.
            LogEvent::NoPreviousInstance => return,
        };
        self.push(line);
    }

    pub fn apply(&mut self, event: LogEvent) {
        match event {
            LogEvent::Line(line) => self.append_line(line),
            // The lines stay, with the marker after them, rather than giving
            // the panel over to a message (`completed-pod-logs`).
            // A panel already taken over by a failure keeps showing it.
            LogEvent::Ended => {
                if self.state == StreamState::Live {
                    self.state = StreamState::Ended;
                }
            }
            LogEvent::RequestFailed { message, detail } => {
                self.state = StreamState::TakenOver {
                    message: format!("Couldn't start log stream: {message}"),
                    detail: Some(detail),
                };
            }
            LogEvent::NoPreviousInstance => {
                self.state = StreamState::TakenOver {
                    message: NO_PREVIOUS_INSTANCE.into(),
                    detail: None,
                };
            }
        }
    }
}

#[cfg(test)]
mod tests;
