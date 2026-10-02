//! The focus indicator every panel draws (`cluster-picker-and-navigation` 12.1): a
//! ring in the accent colour around the panel's content while focus is anywhere
//! inside it. The tab's title underline (`title::title_element`) marks the same
//! state, but at a tab's width it's easy to miss; the ring is what reads at a glance.
//!
//! This only draws focus. Which activations hand a panel focus is the window's and
//! the dock's business, not this module's.

use gpui_kit::*;

/// Rings a panel's root element while `focused`. Pass
/// `focus_handle.contains_focused(..)`, not `is_focused`, so the ring stays lit
/// while a row or an input inside the panel holds focus.
pub(crate) trait FocusRing: Styled + InteractiveElement + Sized {
    /// Draws the ring, and tags the element `{name}-focused` or `{name}-unfocused`
    /// for tests. The ring's width is reserved while unfocused too, so moving
    /// focus between panels doesn't shift their content.
    fn focus_ring(self, name: &'static str, focused: bool, cx: &App) -> Self {
        self.border_2()
            .border_color(ring_color(focused, crate::ui::style::accent(cx)))
            .debug_selector(move || {
                let state = if focused { "focused" } else { "unfocused" };
                format!("{name}-{state}")
            })
    }
}

impl<E: Styled + InteractiveElement> FocusRing for E {}

/// `accent` while focused, otherwise nothing visible.
fn ring_color(focused: bool, accent: Hsla) -> Hsla {
    if focused { accent } else { transparent_black() }
}

#[cfg(test)]
mod tests {
    use super::ring_color;

    #[test]
    fn only_a_focused_panel_shows_its_ring() {
        let accent = gpui_kit::hsla(0.6, 0.9, 0.6, 1.);
        assert_eq!(ring_color(true, accent), accent);
        assert_eq!(ring_color(false, accent).a, 0.);
    }
}
