//! Confirmation prose that names objects (`k9s-remaining-keybindings`): every
//! dialog that asks before losing something - Delete, Close Window, Close
//! Group - says *what* in a sentence, and the names in it are set apart from
//! the sentence around them. Each name is quoted and drawn as a run of its
//! own, in the code font and the accent colour, so `web-0` in "Delete pod
//! “web-0” in default?" reads as the object, not as part of the question.
//!
//! The sentence stays one wrapping text element: names are highlighted ranges
//! of it, resolved at layout over whatever style the dialog inherits, rather
//! than separate elements that would break the line. Owns the text and its
//! drawing; which dialog asks, and what confirming does, is the caller's.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use std::ops::Range;

/// The quotes around a name.
const OPEN_QUOTE: &str = "\u{201c}";
const CLOSE_QUOTE: &str = "\u{201d}";

/// A sentence with object names in it: plain text, and the byte ranges of
/// the names (quotes excluded), in order and never overlapping.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ConfirmText {
    text: String,
    names: Vec<Range<usize>>,
}

impl ConfirmText {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Appends `text` as it is.
    pub(crate) fn text(mut self, text: &str) -> Self {
        self.text.push_str(text);
        self
    }

    /// Appends `name`, quoted and styled as a name.
    pub(crate) fn name(mut self, name: &str) -> Self {
        self.text.push_str(OPEN_QUOTE);
        let start = self.text.len();
        self.text.push_str(name);
        self.names.push(start..self.text.len());
        self.text.push_str(CLOSE_QUOTE);
        self
    }

    /// Appends each of `names`, separated by ", ".
    pub(crate) fn names<S: AsRef<str>>(self, names: &[S]) -> Self {
        names.iter().enumerate().fold(self, |this, (ix, name)| {
            let this = if ix == 0 { this } else { this.text(", ") };
            this.name(name.as_ref())
        })
    }

    /// Appends `other`, names and all.
    pub(crate) fn append(mut self, other: ConfirmText) -> Self {
        let offset = self.text.len();
        self.text.push_str(&other.text);
        self.names.extend(
            other
                .names
                .into_iter()
                .map(|range| range.start + offset..range.end + offset),
        );
        self
    }

    /// The sentence as it reads, quotes included.
    #[cfg(test)]
    pub(crate) fn plain(&self) -> &str {
        &self.text
    }

    /// The names in the sentence, in order.
    #[cfg(test)]
    pub(crate) fn name_list(&self) -> Vec<&str> {
        self.names
            .iter()
            .map(|range| &self.text[range.clone()])
            .collect()
    }

    /// Each name's range and how it is drawn: the accent colour. The code
    /// font is [`Self::render`]'s family override, which a highlight can't
    /// carry.
    fn highlights(&self, cx: &App) -> Vec<(Range<usize>, HighlightStyle)> {
        let accent = crate::ui::style::accent(cx);
        self.names
            .iter()
            .map(|range| {
                let style = HighlightStyle {
                    color: Some(accent),
                    ..HighlightStyle::default()
                };
                (range.clone(), style)
            })
            .collect()
    }

    /// The sentence as one wrapping text element, each name a run of its own
    /// in the code font and the accent colour.
    pub(crate) fn render(&self, cx: &App) -> AnyElement {
        let code = cx.theme().mono_font_family.clone();
        let text = StyledText::new(self.text.clone())
            .with_highlights(self.highlights(cx))
            .with_font_family_overrides(
                self.names
                    .iter()
                    .map(|range| (range.clone(), code.clone()))
                    .collect::<Vec<_>>(),
            );
        // The rest of the sentence keeps the dialog's own (frame) font: only
        // the name ranges are overridden.
        div()
            .debug_selector(|| CONFIRM_TEXT_ID.into())
            .child(text)
            .into_any_element()
    }
}

impl From<&str> for ConfirmText {
    fn from(text: &str) -> Self {
        Self::new().text(text)
    }
}

/// The element a confirmation's sentence is drawn in.
pub(crate) const CONFIRM_TEXT_ID: &str = "confirm-text";

#[cfg(test)]
mod tests;
