//! The picker's sizing constants and static chrome: the card shell ([`card`]), the
//! header row ([`header`]), and the window-minimum
//! derivation ([`MIN_WINDOW_SIZE`]) that sums them so the picker never clips. `render`
//! (in [`super::render`]) assembles these into the full view.

use super::*;

/// The picker card's narrowest width, matching the app's command-palette chrome.
const CARD_WIDTH: f32 = 480.;

/// The widest the card grows with its window: room for a long generated context
/// name (`gke_<project>_<region>_<cluster>`) beside its tunnel selector, without
/// the card stretching across a wide window.
const CARD_MAX_WIDTH: f32 = 800.;

/// The card's debug selector, for tests.
pub(crate) const CARD_SELECTOR: &str = "picker-card";

/// A centered card matching the app's command-palette chrome (`popover`
/// surface, `border` outline, `shadow_lg`), so the picker reads as part of
/// the same design system rather than a bespoke first-run screen.
///
/// It fills the column [`super::render`] centres it in, between [`CARD_WIDTH`] and
/// [`CARD_MAX_WIDTH`], so a wider window gives long context names more room.
pub(super) fn card(cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .debug_selector(|| CARD_SELECTOR.into())
        .w_full()
        .min_w(px(CARD_WIDTH))
        .max_w(px(CARD_MAX_WIDTH))
        .rounded_lg()
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
        .shadow_lg()
        .flex()
        .flex_col()
        .gap(crate::ui::space::spacing(cx).section_gap)
        // Half again the panel inset: the card floats alone in its window.
        .p(crate::ui::space::spacing(cx).panel_inset * 1.5)
}

/// Displayed width of the logo, in logical pixels. The embedded asset is 1241px
/// wide; [`super::logo`] resamples it to this width's device pixels.
pub(super) const LOGO_WIDTH: f32 = 192.;

/// Displayed height, derived from the asset's 1241x1183 geometry so the mark is
/// never stretched.
pub(super) const LOGO_HEIGHT: f32 = 183.;

// The picker's own layout, in logical pixels, as a sum the window minimum can be
// derived from. These track the tailwind-style spacing the render uses (`p_6`,
// `gap_4`, `gap_6` are 1.5rem/1rem/1.5rem at 16px per rem), and the `Command`
// list's default `max_h` of 18.75rem. Deriving the minimum from them rather than
// hardcoding a number is the point: doubling the logo above would otherwise have
// silently started clipping the picker.

/// Card padding top and bottom, plus the header row and the gap beneath it.
const CARD_CHROME_HEIGHT: f32 = 48. + 48. + 16.;

/// `Command`'s list at its `max_h` cap - the tallest it gets, however many
/// contexts the kubeconfig has. Sizing for the cap rather than a typical list is
/// what keeps the picker from clipping on a machine with many contexts.
const CARD_LIST_MAX_HEIGHT: f32 = 300.;

/// Gap between the logo and the card (`gap_6`).
const PICKER_CONTENT_GAP: f32 = 24.;

/// Breathing room either side of the card, so it never sits flush against the
/// window edge.
pub(super) const PICKER_SIDE_MARGIN: f32 = 40.;

/// Minimum window size that fits the picker without clipping.
///
/// Set at window creation and not adjustable afterwards - gpui exposes
/// `window_min_size` only on `WindowOptions`, with no setter on `Window`. A
/// picker window is the same window the workspace is later shown in, so this
/// also floors the workspace: 560x619 against a 1024x768 default leaves the
/// dock room to breathe while still letting a user shrink a long way down.
pub const MIN_WINDOW_SIZE: Size<Pixels> = Size {
    width: px(CARD_WIDTH + 2. * PICKER_SIDE_MARGIN),
    height: px(LOGO_HEIGHT + PICKER_CONTENT_GAP + CARD_CHROME_HEIGHT + CARD_LIST_MAX_HEIGHT),
};

pub(super) fn header(cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .flex()
        .items_center()
        .gap_3()
        .child(
            Icon::new(IconName::Server)
                .size(px(28.))
                .text_color(theme.accent),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .child(div().text_lg().font_semibold().child("Select a cluster"))
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("Choose a kubeconfig context to connect to"),
                ),
        )
}

#[cfg(test)]
mod tests;
