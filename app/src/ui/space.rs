//! The app's spacing scale (`visual-language` spec, "Spacing comes from one
//! scale"): panel insets, card padding, table row height and the gaps between
//! sections and controls, as one set of tokens every view draws from instead
//! of literal pixels.
//!
//! The tokens scale with the text-size preference, so larger text doesn't
//! crowd. That preference lives elsewhere (`visual-refresh-typography-spacing`
//! section 4); this module only reads its factor through [`TextScale`], which
//! is 1.0 until something sets it.
//!
//! Each base value is one step roomier than what panels used before the
//! scale existed, noted beside it.

use gpui_kit::*;

/// Content's distance from a panel's edge. Was 12 (`p_3`).
const PANEL_INSET: f32 = 16.;
/// Content's distance from a card's edge. Was 8 (`p_2`).
const CARD_PADDING: f32 = 12.;
/// A table row's height. Was gpui-component's default, 32.
const ROW_HEIGHT: f32 = 36.;
/// The space between sections of a panel. Was 12 (`pt_3`).
const SECTION_GAP: f32 = 16.;
/// The space between neighbouring controls. Was 8 (`gap_2`).
const CONTROL_GAP: f32 = 10.;

/// The text-size factor the spacing tokens scale by: 1.0 is the default size.
/// An app global; while unset, [`TextScale::current`] reads it as 1.0.
///
/// This type only promises the factor is a positive, finite number. The
/// preference that sets it owns its range and steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextScale(f32);

impl Global for TextScale {}

impl Default for TextScale {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl TextScale {
    /// The default text size.
    pub const DEFAULT: Self = Self(1.0);

    // UNWIRED: the text-size preference (`visual-refresh-typography-spacing`
    // section 4) builds its scale with this; until then only tests do.
    #[allow(dead_code)]
    /// A scale of `factor` (1.0 is the default size), or `None` for a factor
    /// that isn't a positive, finite number.
    pub fn new(factor: f32) -> Option<Self> {
        (factor.is_finite() && factor > 0.).then_some(Self(factor))
    }

    pub fn factor(self) -> f32 {
        self.0
    }

    /// The app's current scale: the one last [`set`](Self::set), else
    /// [`TextScale::DEFAULT`].
    pub fn current(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or_default()
    }

    // UNWIRED: see `new`.
    #[allow(dead_code)]
    /// Makes this the app's scale and redraws every window, so open windows
    /// take the new spacing without a restart.
    pub fn set(self, cx: &mut App) {
        cx.set_global(self);
        cx.refresh_windows();
    }
}

/// The spacing tokens at one text scale, each rounded to a whole pixel so
/// edges don't land between pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spacing {
    /// Content's minimum distance from a panel's edge.
    pub panel_inset: Pixels,
    /// Content's minimum distance from a card's edge.
    pub card_padding: Pixels,
    /// A table row's height.
    pub row_height: Pixels,
    /// The space between sections of a panel.
    pub section_gap: Pixels,
    /// The space between neighbouring controls.
    pub control_gap: Pixels,
}

impl Spacing {
    /// The tokens at `scale`.
    pub fn at(scale: TextScale) -> Self {
        let scaled = |base: f32| px((base * scale.factor()).round());
        Self {
            panel_inset: scaled(PANEL_INSET),
            card_padding: scaled(CARD_PADDING),
            row_height: scaled(ROW_HEIGHT),
            section_gap: scaled(SECTION_GAP),
            control_gap: scaled(CONTROL_GAP),
        }
    }
}

/// The tokens at the app's current text scale. What views call.
pub fn spacing(cx: &App) -> Spacing {
    Spacing::at(TextScale::current(cx))
}

#[cfg(test)]
mod tests;
