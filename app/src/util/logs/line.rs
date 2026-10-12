//! One line of a Logs panel ([`LogLine`]): its text, and - for a panel
//! following several containers (#150) - the [`LineSource`] it came from,
//! kept apart from the text so the tag can be drawn in its pod's colour
//! (`selector-logs`). GPUI-free, like the view-model that holds these.

use std::fmt;
use std::sync::Arc;

/// Between a tagged line's source and its text, where a line is shown or
/// compared as one string (#150).
pub const SOURCE_SEPARATOR: &str = "\u{2502}";

/// Where a tagged line came from: the tag drawn before it - the pod, or
/// `pod/container` when the pod streams several - and the pod whose colour the
/// tag takes. `Arc`s: every line of one stream shares its source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineSource {
    tag: Arc<str>,
    pod: Arc<str>,
}

impl LineSource {
    pub fn new(tag: impl Into<Arc<str>>, pod: impl Into<Arc<str>>) -> Self {
        Self {
            tag: tag.into(),
            pod: pod.into(),
        }
    }

    pub fn tag(&self) -> &str {
        &self.tag
    }

    pub fn pod(&self) -> &str {
        &self.pod
    }
}

/// A line the panel shows: its text, and its source when it came from one of
/// several streams.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    source: Option<LineSource>,
    text: String,
}

impl LogLine {
    /// A single stream's line, with no tag.
    pub fn plain(text: String) -> Self {
        Self { source: None, text }
    }

    /// One of several streams' lines, tagged with where it came from.
    pub fn tagged(source: LineSource, text: String) -> Self {
        Self {
            source: Some(source),
            text,
        }
    }

    pub fn source(&self) -> Option<&LineSource> {
        self.source.as_ref()
    }

    /// The line's own text, without its tag. Only tests read it apart: the
    /// panel shows and copies the whole line ([`fmt::Display`]).
    #[cfg(test)]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether this line reads as `shown`: its text, after `tag │ ` when it
    /// has a source - compared in place, without building that string.
    fn reads_as(&self, shown: &str) -> bool {
        match &self.source {
            None => self.text == shown,
            Some(source) => {
                shown
                    .strip_prefix(source.tag())
                    .and_then(|rest| rest.strip_prefix(' '))
                    .and_then(|rest| rest.strip_prefix(SOURCE_SEPARATOR))
                    .and_then(|rest| rest.strip_prefix(' '))
                    == Some(self.text.as_str())
            }
        }
    }
}

/// The line as one string: `tag │ text`, or the text alone.
impl fmt::Display for LogLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.source {
            None => f.write_str(&self.text),
            Some(source) => write!(f, "{} {SOURCE_SEPARATOR} {}", source.tag(), self.text),
        }
    }
}

impl PartialEq<str> for LogLine {
    fn eq(&self, other: &str) -> bool {
        self.reads_as(other)
    }
}

impl PartialEq<&str> for LogLine {
    fn eq(&self, other: &&str) -> bool {
        self.reads_as(other)
    }
}

impl PartialEq<String> for LogLine {
    fn eq(&self, other: &String) -> bool {
        self.reads_as(other)
    }
}

/// Which of [`crate::consts::LABEL_LOGS_POD_HUES`] `pod`'s tag is drawn in: a
/// function of the name alone, so a pod keeps its colour across restarts,
/// re-syncs and reopened panels with no assignment kept anywhere. FNV-1a
/// rather than `std`'s hasher, whose output isn't promised to stay the same
/// from one Rust release to the next.
pub fn pod_colour(pod: &str) -> usize {
    const OFFSET: u32 = 0x811c_9dc5;
    const PRIME: u32 = 0x0100_0193;
    let hash = pod.bytes().fold(OFFSET, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(PRIME)
    });
    hash as usize % crate::consts::LABEL_LOGS_POD_HUES.len()
}

#[cfg(test)]
mod tests;
