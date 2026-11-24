//! One line of a Logs panel ([`LogLine`]): its text, and - for a panel
//! following several containers (#150) - the [`LineSource`] it came from,
//! kept apart from the text so the tag can be drawn in its pod's colour
//! (`selector-logs`). GPUI-free, like the view-model that holds these.

use super::ansi::Span;
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

/// A line the panel shows: its text, with any ANSI escapes taken out (#309),
/// the styled runs of that text, and its source when it came from one of
/// several streams.
// Tests count clones (`clone_count`), so a test can show that drawing the
// panel copies no lines (#378); everywhere else `Clone` is derived.
#[cfg_attr(not(test), derive(Clone))]
#[derive(Debug, PartialEq, Eq)]
pub struct LogLine {
    source: Option<LineSource>,
    text: String,
    /// `None` for a line plain throughout - most of them. Shared, so a line's
    /// copy doesn't copy its styling.
    spans: Option<Arc<[Span]>>,
    /// The panel's own words, not a stream's output - a tagged stream's end
    /// or failure - drawn apart from the log (#360).
    notice: bool,
}

#[cfg(test)]
thread_local! {
    static CLONES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
impl Clone for LogLine {
    fn clone(&self) -> Self {
        CLONES.with(|clones| clones.set(clones.get() + 1));
        Self {
            source: self.source.clone(),
            text: self.text.clone(),
            spans: self.spans.clone(),
            notice: self.notice,
        }
    }
}

/// How many [`LogLine`]s this thread has cloned so far.
#[cfg(test)]
pub fn clone_count() -> usize {
    CLONES.with(std::cell::Cell::get)
}

impl LogLine {
    /// A single stream's line, with no tag and no styling.
    #[cfg(test)]
    pub fn plain(text: String) -> Self {
        Self::styled(None, text, Vec::new())
    }

    /// One of several streams' lines, tagged with where it came from, with no
    /// styling.
    pub fn tagged(source: LineSource, text: String) -> Self {
        Self::styled(Some(source), text, Vec::new())
    }

    /// A line whose escapes `ansi::parse` took out: its plain `text` and the
    /// `spans` styling it.
    pub fn styled(source: Option<LineSource>, text: String, spans: Vec<Span>) -> Self {
        Self {
            source,
            text,
            spans: (!spans.is_empty()).then(|| spans.into()),
            notice: false,
        }
    }

    /// The panel's own line about `source`'s stream - that it ended, or
    /// failed - tagged like its lines but drawn as a notice (#360).
    pub fn notice(source: LineSource, text: String) -> Self {
        Self {
            notice: true,
            ..Self::tagged(source, text)
        }
    }

    /// Whether this is the panel's own line rather than a stream's.
    pub fn is_notice(&self) -> bool {
        self.notice
    }

    pub fn source(&self) -> Option<&LineSource> {
        self.source.as_ref()
    }

    /// The styled runs of the line's text, by byte range in it - not in the
    /// tagged line [`fmt::Display`] shows. Empty for a plain line.
    pub fn spans(&self) -> &[Span] {
        self.spans.as_deref().unwrap_or_default()
    }

    /// The line's own text, without its tag or escapes. Only tests read it
    /// apart: the panel shows and copies the whole line ([`fmt::Display`]).
    #[cfg(test)]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Where the line's own text starts in the line [`fmt::Display`] shows:
    /// after `tag │ `, or at 0 with no tag.
    pub fn text_offset(&self) -> usize {
        self.source.as_ref().map_or(0, |source| {
            source.tag().len() + 1 + SOURCE_SEPARATOR.len() + 1
        })
    }

    /// How many characters the line shows ([`fmt::Display`]): its width in
    /// the monospaced code font, counted without building the string. A
    /// proxy: a wide glyph (CJK, emoji) or a tab takes more than one column,
    /// so a line of those can be wider on screen than its count says.
    pub fn width(&self) -> usize {
        let tag = self.source.as_ref().map_or(0, |source| {
            source.tag().chars().count() + 1 + SOURCE_SEPARATOR.chars().count() + 1
        });
        tag + self.text.chars().count()
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
