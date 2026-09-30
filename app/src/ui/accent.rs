//! The accent colour Fernrohr marks "this is where you are" with - today the
//! focused panel's tab underline.
//!
//! On macOS that is the user's own accent colour from System Settings
//! (`NSColor.controlAccentColor`). Other platforms have no such lookup here yet,
//! so they fall back to the theme's `blue` - not `primary`, which in the default
//! themes is the selected tab's own text colour and would mark nothing.
//!
//! The system colour is read into a global rather than per frame: it is an
//! AppKit call, and titles render every frame. [`refresh`] re-reads it at
//! startup, on every light/dark change, and whenever a window becomes active -
//! which is how a change made in System Settings is picked up the moment the
//! user switches back.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;

/// The system accent colour as last read, or `None` where the platform offers
/// none or it could not be converted to sRGB.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SystemAccent(Option<Hsla>);

impl Global for SystemAccent {}

/// The colour to mark the focused panel with: the system accent when there is
/// one, otherwise the theme's `blue`.
pub fn focus_accent(cx: &App) -> Hsla {
    cx.try_global::<SystemAccent>()
        .and_then(|accent| accent.0)
        .unwrap_or(cx.theme().blue)
}

/// Re-reads the system accent. Returns whether it changed, so a caller knows
/// whether its window needs redrawing.
pub fn refresh(cx: &mut App) -> bool {
    let accent = SystemAccent(system_accent());
    let changed = cx.try_global::<SystemAccent>() != Some(&accent);
    cx.set_global(accent);
    changed
}

/// Re-reads the accent whenever `window` becomes active, redrawing it if the
/// colour changed. Tied to the view behind `cx`, so it stops with that view.
pub fn watch_activation<T: 'static>(window: &mut Window, cx: &mut Context<T>) {
    cx.observe_window_activation(window, |_, window, cx| {
        if window.is_window_active() && refresh(cx) {
            window.refresh();
        }
    })
    .detach();
}

#[cfg(target_os = "macos")]
fn system_accent() -> Option<Hsla> {
    use objc2_app_kit::{NSColor, NSColorSpace};

    // A dynamic colour in an unspecified colour space; converting it resolves it
    // against the app's current appearance. Catalog or pattern colours can
    // refuse the conversion, which is the `None` fallback.
    let accent =
        NSColor::controlAccentColor().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    Some(
        Rgba {
            r: accent.redComponent() as f32,
            g: accent.greenComponent() as f32,
            b: accent.blueComponent() as f32,
            a: accent.alphaComponent() as f32,
        }
        .into(),
    )
}

#[cfg(not(target_os = "macos"))]
fn system_accent() -> Option<Hsla> {
    None
}

#[cfg(test)]
mod tests {
    use super::{SystemAccent, focus_accent};
    use gpui_kit::component::ActiveTheme as _;
    use gpui_kit::{BorrowAppContext as _, TestAppContext, hsla};

    /// Without a system accent the theme's blue marks focus, so Linux and
    /// Windows still get a visible mark.
    #[gpui_kit::test]
    fn without_a_system_accent_the_theme_blue_is_used(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(SystemAccent(None));
            assert_eq!(focus_accent(cx), cx.theme().blue);
        });
    }

    /// A system accent, when there is one, wins over the theme.
    #[gpui_kit::test]
    fn a_system_accent_wins_over_the_theme(cx: &mut TestAppContext) {
        let accent = hsla(0.8, 0.7, 0.5, 1.);
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(SystemAccent(Some(accent)));
            assert_eq!(focus_accent(cx), accent);
            cx.update_global::<SystemAccent, _>(|stored, _| stored.0 = None);
            assert_eq!(focus_accent(cx), cx.theme().blue);
        });
    }

    /// macOS always has an accent colour, and AppKit converts it to an opaque
    /// sRGB colour - so on this platform the lookup never silently falls back.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_reads_an_opaque_system_accent() {
        let accent = super::system_accent().expect("macOS has an accent colour");
        assert_eq!(accent.a, 1.);
    }
}
