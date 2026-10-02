//! The app's three type roles (`typography` spec, `visual-refresh-typography-
//! spacing` design.md): *frame* for the application's chrome, *data* for what
//! a panel shows, *code* for YAML and logs.
//!
//! - **Frame** is Adamina, set as gpui-component's theme `font_family`. `Root`
//!   applies that to the whole window, so panel titles, tabs, buttons, menus,
//!   dialogs and the context and status bars take it without asking.
//! - **Data** is Manrope. A view that draws data - a table cell, a field row, a
//!   chip, a card - opts in with [`TypeRole::data_font`]. It goes on those leaf
//!   elements rather than on a whole panel, so a section heading inside a
//!   detail view keeps the frame font.
//! - **Code** is the theme's `mono_font_family` (Monaco, or an installed
//!   fallback; see `ui::theme`).
//!
//! Adamina and Manrope are bundled with the app (SIL Open Font License, see
//! `assets/fonts/*-OFL.txt`); neither is a system font on macOS or Linux.
//! Adamina ships one weight, so frame hierarchy comes from size and colour,
//! not bold. See [`BUNDLED_FONTS`] for why every file is a static face.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;

/// The frame role's family.
pub(crate) const FRAME_FAMILY: &str = "Adamina";
/// The data role's family.
pub(crate) const DATA_FAMILY: &str = "Manrope";

/// Every font file the app bundles, registered with the text system at
/// startup by `ui::theme::init`.
///
/// All static, one file per weight - never a variable font. GPUI has no
/// font-variation support: it picks one face per family and weight, so a
/// variable file draws every weight at its default instance (Manrope's is
/// ExtraLight, which is what made data text look thin; `visual-refresh-
/// typography-spacing` 1.1). Manrope ships the weights the app asks for -
/// regular, medium, semibold, bold - and Adamina has only Regular.
pub(crate) const BUNDLED_FONTS: &[&[u8]] = &[
    include_bytes!("../../assets/fonts/Adamina-Regular.ttf"),
    include_bytes!("../../assets/fonts/Manrope-Regular.ttf"),
    include_bytes!("../../assets/fonts/Manrope-Medium.ttf"),
    include_bytes!("../../assets/fonts/Manrope-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/Manrope-Bold.ttf"),
];

/// Picks a type role for an element. The frame role is the window's default,
/// so [`TypeRole::frame_font`] is only for frame text nested inside a data
/// element.
pub trait TypeRole: Styled + Sized {
    /// Frame text: the theme's UI family.
    fn frame_font(self, cx: &App) -> Self {
        self.font_family(cx.theme().font_family.clone())
    }

    /// Data text: table cells, field labels and values, chips and cards.
    fn data_font(self) -> Self {
        self.font_family(DATA_FAMILY)
    }

    /// Code text: YAML and logs.
    fn code_font(self, cx: &App) -> Self {
        self.font_family(cx.theme().mono_font_family.clone())
    }
}

impl<T: Styled> TypeRole for T {}

#[cfg(test)]
pub(crate) mod recorder;
#[cfg(test)]
mod tests;
