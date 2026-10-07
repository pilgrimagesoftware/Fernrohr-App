//! The accent colour Fernrohr marks "this is where you are" and "you can go
//! here" with - the focused panel's tab underline, and followable links.
//!
//! That is the user's own accent colour: on macOS from System Settings
//! (`NSColor.controlAccentColor`), on Linux from the XDG desktop portal's
//! `org.freedesktop.appearance` `accent-color` ([`portal`]), and on Windows
//! the DWM colorization colour ([`dwm`]) - Fernrohr#100. Where there is none,
//! the theme's `blue` stands in - not `primary`, which in the default themes
//! is the selected tab's own text colour and would mark nothing. Either way
//! `ui::style` adjusts it to the contrast floor.
//!
//! The system colour is read into a global rather than per frame: titles
//! render every frame. [`refresh`] re-reads it at startup, on every light/dark
//! change, and whenever a window becomes active - which is how a change made in
//! the system settings is picked up the moment the user switches back. The
//! portal is a D-Bus call that can stall, so on Linux `refresh` applies the
//! last reading and asks for a new one off the main thread, redrawing the
//! windows when it lands.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;

#[cfg(windows)]
mod dwm;
#[cfg(target_os = "linux")]
mod portal;

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

/// Re-reads the system accent and re-applies the theme overrides that use it
/// ([`crate::ui::style::apply_overrides`]). Runs after every theme change -
/// which resets those overrides - and on window activation. Returns whether
/// the accent changed, so a caller knows whether its window needs redrawing.
pub fn refresh(cx: &mut App) -> bool {
    #[cfg(target_os = "linux")]
    portal::request(cx);
    apply(system_accent(cx), cx)
}

/// Stores `accent` as the system accent and re-applies the overrides that
/// use it. Whether it changed.
fn apply(accent: Option<Hsla>, cx: &mut App) -> bool {
    let accent = SystemAccent(accent);
    let changed = cx.try_global::<SystemAccent>() != Some(&accent);
    cx.set_global(accent);
    crate::ui::style::apply_overrides(cx);
    changed
}

/// Test-only: stands in for the system accent, then re-applies the overrides.
#[cfg(test)]
pub(crate) fn set_for_test(accent: Option<Hsla>, cx: &mut App) {
    cx.set_global(SystemAccent(accent));
    crate::ui::style::apply_overrides(cx);
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

/// The system accent as this platform has it now - on Linux, as last read
/// from the portal.
#[cfg(target_os = "macos")]
fn system_accent(_cx: &App) -> Option<Hsla> {
    read_macos()
}

#[cfg(target_os = "linux")]
fn system_accent(cx: &App) -> Option<Hsla> {
    portal::cached(cx)
}

#[cfg(windows)]
fn system_accent(_cx: &App) -> Option<Hsla> {
    dwm::colorization().map(from_colorization)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn system_accent(_cx: &App) -> Option<Hsla> {
    None
}

/// The portal's accent, sRGB components each in `0..=1`; any component outside
/// that range is the portal's way of saying the user has set none.
#[cfg(any(test, target_os = "linux"))]
fn from_unit_rgb(red: f64, green: f64, blue: f64) -> Option<Hsla> {
    let unit = |value: f64| (0.0..=1.0).contains(&value).then_some(value as f32);
    Some(
        Rgba {
            r: unit(red)?,
            g: unit(green)?,
            b: unit(blue)?,
            a: 1.,
        }
        .into(),
    )
}

/// A DWM colorization colour, `0xAARRGGBB`, as an opaque colour: its alpha is
/// how DWM blends the frame, not part of the colour the user picked.
#[cfg(any(test, windows))]
fn from_colorization(argb: u32) -> Hsla {
    let channel = |shift: u32| ((argb >> shift) & 0xff) as f32 / 255.;
    Rgba {
        r: channel(16),
        g: channel(8),
        b: channel(0),
        a: 1.,
    }
    .into()
}

#[cfg(target_os = "macos")]
fn read_macos() -> Option<Hsla> {
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

#[cfg(test)]
mod tests {
    use super::{SystemAccent, focus_accent, from_colorization, from_unit_rgb};
    use gpui_kit::component::ActiveTheme as _;
    use gpui_kit::{BorrowAppContext as _, TestAppContext, hsla};

    /// Without a system accent the theme's blue marks focus, so Linux and
    /// Windows still get a visible mark.
    #[gpui_kit::test]
    fn without_a_system_accent_the_theme_blue_is_used(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::util::test_ui::init(cx);
            cx.set_global(SystemAccent(None));
            assert_eq!(focus_accent(cx), cx.theme().blue);
        });
    }

    /// A system accent, when there is one, wins over the theme.
    #[gpui_kit::test]
    fn a_system_accent_wins_over_the_theme(cx: &mut TestAppContext) {
        let accent = hsla(0.8, 0.7, 0.5, 1.);
        cx.update(|cx| {
            crate::util::test_ui::init(cx);
            cx.set_global(SystemAccent(Some(accent)));
            assert_eq!(focus_accent(cx), accent);
            cx.update_global::<SystemAccent, _>(|stored, _| stored.0 = None);
            assert_eq!(focus_accent(cx), cx.theme().blue);
        });
    }

    /// Fernrohr#100: the portal's accent in range is that colour, opaque; out
    /// of range - the portal's "unset" - is none.
    #[test]
    fn a_portal_accent_in_range_is_used_and_out_of_range_is_unset() {
        let accent = from_unit_rgb(0.21, 0.52, 0.89).expect("in range");
        let rgba = gpui_kit::Rgba::from(accent);
        assert!((rgba.r - 0.21).abs() < 0.01 && (rgba.b - 0.89).abs() < 0.01);
        assert_eq!(rgba.a, 1.);
        assert_eq!(from_unit_rgb(-1., -1., -1.), None);
        assert_eq!(from_unit_rgb(0.5, 1.5, 0.5), None);
    }

    /// Fernrohr#100: a DWM colorization colour's channels, its alpha dropped.
    #[test]
    fn a_dwm_colorization_reads_its_rgb_and_drops_its_alpha() {
        let rgba = gpui_kit::Rgba::from(from_colorization(0x80_00_78_d4));
        assert!(rgba.r.abs() < 0.01);
        assert!((rgba.g - 0x78 as f32 / 255.).abs() < 0.01);
        assert!((rgba.b - 0xd4 as f32 / 255.).abs() < 0.01);
        assert_eq!(rgba.a, 1.);
    }

    /// macOS always has an accent colour, and AppKit converts it to an opaque
    /// sRGB colour - so on this platform the lookup never silently falls back.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_reads_an_opaque_system_accent() {
        let accent = super::read_macos().expect("macOS has an accent colour");
        assert_eq!(accent.a, 1.);
    }
}
