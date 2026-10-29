//! A detail panel header's row: its title and its key hints, and how they share
//! the width. The title takes the width it needs first, pushing the hints over,
//! and the hints fill what's left, wrapping onto further rows. Only when the
//! hints would be squeezed below [`HINTS_MIN_REMS`] does the title give way,
//! ellipsizing down to [`TITLE_MIN_REMS`].
//!
//! A row too narrow for both floors side by side stacks them instead: the title
//! on its own line, ellipsizing at the row's edge, and the hints beneath it,
//! wrapping and collapsing within the row. Nothing in the header is ever wider
//! than the row.
//!
//! Shared by the pod detail and object viewer headers.

use crate::ui::key_hint_row::KeyHintRow;
use gpui_kit::*;

/// The title's floor beside the hints: a few characters of the name stay
/// readable.
const TITLE_MIN_REMS: f32 = 8.;
/// The hints' floor beside the title: room for the widest hint on its own row.
const HINTS_MIN_REMS: f32 = 14.;

/// A detail header's row: `title` on the left, `hints` on the right, or
/// stacked when the row is narrower than both floors. Style `hints` as usual
/// (text size, colour, hint gap); the row sets how it's sized.
#[derive(IntoElement)]
pub struct HeaderRow {
    id: SharedString,
    title: AnyElement,
    hints: KeyHintRow,
    gap: Option<Pixels>,
}

impl HeaderRow {
    /// `id` keeps whether the row is stacked between frames, so it must be
    /// unique among the headers drawn in one view.
    pub fn new(id: impl Into<SharedString>, title: impl IntoElement, hints: KeyHintRow) -> Self {
        Self {
            id: id.into(),
            title: title.into_any_element(),
            hints,
            gap: None,
        }
    }

    /// The space between the title and the hints. Defaults to the theme's
    /// control gap.
    pub fn gap(mut self, gap: Pixels) -> Self {
        self.gap = Some(gap);
        self
    }
}

/// Whether a row is stacked, kept between frames by `window.use_keyed_state`.
#[derive(Debug, Default)]
struct Layout {
    stacked: bool,
}

/// Whether a row `width` wide stacks: it can't fit both floors and the gap
/// between them on one line. Depends only on the row's own width, which the
/// panel sets, so it can't flicker between the two layouts.
fn stacks(width: Pixels, floors: Pixels, gap: Pixels) -> bool {
    width + px(0.5) < floors + gap
}

impl RenderOnce for HeaderRow {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let layout = window.use_keyed_state(
            ElementId::Name(format!("{} layout", self.id).into()),
            cx,
            |_, _| Layout::default(),
        );
        let stacked = layout.read(cx).stacked;
        let gap = self
            .gap
            .unwrap_or_else(|| crate::ui::space::spacing(cx).control_gap);
        let floors = rems(TITLE_MIN_REMS + HINTS_MIN_REMS).to_pixels(window.rem_size());

        let row = div()
            .relative()
            .flex_1()
            .min_w_0()
            .flex()
            .gap(gap)
            .debug_selector({
                let id = self.id.clone();
                move || row_selector(&id, stacked)
            });
        let row = if stacked {
            // Each on its own line, as wide as the row and no wider.
            row.flex_col()
                .child(div().w_full().min_w_0().child(self.title))
                .child(self.hints.w_full().min_w_0())
        } else {
            row.items_center()
                .justify_between()
                // As wide as the title's full text, shrinking - and so
                // ellipsizing - only once the hints are down to their floor.
                .child(
                    div()
                        .flex_initial()
                        .min_w(rems(TITLE_MIN_REMS))
                        .child(self.title),
                )
                // Whatever the title leaves, down to the hints' floor, right-
                // aligned and wrapping rather than overflowing.
                .child(
                    self.hints
                        .flex_1()
                        .min_w(rems(HINTS_MIN_REMS))
                        .justify_end(),
                )
        };
        row
            // Last, and out of the flow: it spans the row, so its bounds are
            // the row's own width.
            .child(div().absolute().inset_0())
            .on_children_prepainted(move |bounds, _, cx| {
                let Some(row) = bounds.last() else {
                    return;
                };
                let stack = stacks(row.size.width, floors, gap);
                layout.update(cx, |layout, cx| {
                    if layout.stacked != stack {
                        layout.stacked = stack;
                        cx.notify();
                    }
                });
            })
    }
}

/// The debug selector of the header row `id`, which says whether it is
/// stacked, so a test can wait for the layout it expects.
fn row_selector(id: &str, stacked: bool) -> String {
    let layout = if stacked { "stacked" } else { "side by side" };
    format!("header-row {id} {layout}")
}

#[cfg(test)]
mod tests;
