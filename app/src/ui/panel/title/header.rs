//! A detail panel header's two columns - its title and its key hints - and how
//! they share the row: the title takes the width it needs first, pushing the
//! hints over, and the hints fill what's left, wrapping onto further rows. Only
//! when the hints would be squeezed below [`HINTS_MIN_REMS`] does the title give
//! way, ellipsizing down to [`TITLE_MIN_REMS`].
//!
//! Shared by the pod detail and object viewer headers.

use gpui_kit::*;

/// The title's floor: a few characters of the name stay readable.
const TITLE_MIN_REMS: f32 = 8.;
/// The hints' floor: room for the widest hint on its own row.
const HINTS_MIN_REMS: f32 = 14.;

/// The title column: as wide as `title`'s full text, shrinking - and so
/// ellipsizing - only once the hints are down to their floor.
pub fn header_title(title: impl IntoElement) -> Div {
    div()
        .flex_initial()
        .min_w(rems(TITLE_MIN_REMS))
        .child(title)
}

/// The hints column: whatever the title leaves, down to its floor, its hints
/// right-aligned and wrapping onto further rows rather than overflowing.
pub fn header_hints() -> Div {
    div()
        .flex()
        .flex_1()
        .min_w(rems(HINTS_MIN_REMS))
        .flex_wrap()
        .justify_end()
}

#[cfg(test)]
mod tests;
