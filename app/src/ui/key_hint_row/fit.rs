//! Whether a hint row fits in [`MAX_HINT_LINES`] lines (`key-hint-rows` D1):
//! the line count measured from where the hints were laid out, the line count
//! the hints' last measured widths would pack into, and the collapsed flag the
//! two keep, with hysteresis so a row at the boundary doesn't flicker.
//!
//! Pure over pixel values, so the decisions are unit-tested without a window.

use crate::consts::MAX_HINT_LINES;
use gpui_kit::*;
use std::collections::HashMap;

/// A row's fit, kept between frames by `window.use_keyed_state`.
#[derive(Debug, Default)]
pub(super) struct Fit {
    /// Whether the row shows only its essential hints and the overflow control.
    pub(super) collapsed: bool,
    /// Each item's width the last time it was drawn, by item key.
    widths: HashMap<SharedString, Pixels>,
}

/// One layout pass's results: every item's key in row order, the keys of the
/// items drawn this frame, and the bounds of the drawn children - the drawn
/// items, then the overflow control if there was one, then the full-size
/// overlay that measures the row itself.
pub(super) struct Measured<'a> {
    pub(super) all: &'a [SharedString],
    pub(super) drawn: &'a [SharedString],
    pub(super) bounds: &'a [Bounds<Pixels>],
    pub(super) gap: Pixels,
}

impl Fit {
    /// Takes a frame's measurements; returns whether the row should change from
    /// collapsed to expanded or back, which needs a redraw.
    pub(super) fn measure(&mut self, frame: Measured<'_>) -> bool {
        let Some(row) = frame.bounds.last() else {
            return false;
        };
        let Some(items) = frame.bounds.get(..frame.drawn.len()) else {
            return false;
        };
        for (key, bounds) in frame.drawn.iter().zip(items) {
            self.widths.insert(key.clone(), bounds.size.width);
        }
        self.widths.retain(|key, _| frame.all.contains(key));
        let changed = if self.collapsed {
            packed_lines(&self.widths_of(frame.all), frame.gap, row.size.width) <= MAX_HINT_LINES
        } else {
            laid_out_lines(items) > MAX_HINT_LINES
        };
        if changed {
            self.collapsed = !self.collapsed;
        }
        changed
    }

    /// The widths of `keys` in order. An item never drawn - one that appeared
    /// while the row was collapsed - counts as the average of those that were.
    fn widths_of(&self, keys: &[SharedString]) -> Vec<Pixels> {
        let known: Vec<Pixels> = keys
            .iter()
            .filter_map(|key| self.widths.get(key))
            .copied()
            .collect();
        let average = if known.is_empty() {
            px(0.)
        } else {
            known.iter().fold(px(0.), |sum, width| sum + *width) / known.len() as f32
        };
        keys.iter()
            .map(|key| self.widths.get(key).copied().unwrap_or(average))
            .collect()
    }
}

/// How many lines `items` were laid out on. Items on one line overlap
/// vertically - they can differ in height, centred on the line - so an item
/// starts a new line only when its top is at or below the bottom of the line
/// so far.
pub(super) fn laid_out_lines(items: &[Bounds<Pixels>]) -> usize {
    let mut spans: Vec<(Pixels, Pixels)> = items
        .iter()
        .map(|bounds| (bounds.top(), bounds.bottom()))
        .collect();
    spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut lines = 0;
    let mut line_bottom: Option<Pixels> = None;
    for (top, bottom) in spans {
        match line_bottom {
            Some(current) if top < current - px(0.5) => {
                line_bottom = Some(current.max(bottom));
            }
            _ => {
                lines += 1;
                line_bottom = Some(bottom);
            }
        }
    }
    lines
}

/// How many lines items of `widths` take in a wrapping row `available` wide,
/// `gap` apart - the way flexbox wraps them: each line takes items while they
/// fit, and an item wider than the row still gets a line to itself.
pub(super) fn packed_lines(widths: &[Pixels], gap: Pixels, available: Pixels) -> usize {
    let mut lines = 0;
    let mut used = px(0.);
    for &width in widths {
        if lines == 0 || used + gap + width > available + px(0.5) {
            lines += 1;
            used = width;
        } else {
            used = used + gap + width;
        }
    }
    lines
}

/// Which items a row draws, in order: every item but the summaries when
/// expanded; when collapsed, the controls, the essential hints and the
/// summaries, or the first hint when there are none of the latter two, so a
/// collapsed row always keeps one.
pub(super) fn shown(kinds: &[ItemKind], collapsed: bool) -> Vec<bool> {
    if !collapsed {
        return kinds
            .iter()
            .map(|kind| *kind != ItemKind::Summary)
            .collect();
    }
    let any_essential = kinds
        .iter()
        .any(|kind| matches!(kind, ItemKind::EssentialHint | ItemKind::Summary));
    let first_hint = kinds.iter().position(|kind| *kind == ItemKind::Hint);
    kinds
        .iter()
        .enumerate()
        .map(|(index, kind)| match kind {
            ItemKind::Control | ItemKind::EssentialHint | ItemKind::Summary => true,
            ItemKind::Hint => !any_essential && Some(index) == first_hint,
        })
        .collect()
}

/// What an item in a row is, for [`shown`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ItemKind {
    /// Always drawn: an icon button that is a control's mouse route.
    Control,
    /// A hint kept when the row collapses.
    EssentialHint,
    /// A hint the row hides when it collapses.
    Hint,
    /// A hint drawn only while the row is collapsed.
    Summary,
}
