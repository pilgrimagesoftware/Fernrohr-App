//! Applies the user's `UiConfig::theme` preference to gpui-component's global
//! `Theme`, once at startup and again on every live OS appearance change when
//! the preference is `System` - so light/dark mode is a day-one concern
//! rather than something bolted on after the picker and main window exist.
//!
//! Also registers the bundled fonts and sets the theme's two font fields from
//! the type roles (`ui::typography`): the frame role, Adamina, as
//! `font_family`, and the code role, Monaco (with a real installed fallback),
//! as `mono_font_family`. Both are reasserted on every appearance change
//! alongside the theme mode, so a light/dark flip can't revert either field to
//! gpui-component's own defaults.

use crate::config::ui::Theme as ThemePreference;
use crate::ui::typography::{BUNDLED_FONTS, FRAME_FAMILY};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
use std::borrow::Cow;

impl Global for ThemePreference {}

/// Monospace families tried, in order, when Monaco isn't installed - not
/// bundled (Monaco is a macOS system font, and shipping a full monospace
/// font just for the non-macOS fallback case is disproportionate to this
/// change); Linux is the documented lower-maturity target already, and this
/// still guarantees real monospaced text rather than an unstyled default.
const MONO_FONT_ALTERNATES: &[&str] = &[
    "Monaco",
    "Menlo",
    "Cascadia Mono",
    "Noto Sans Mono",
    "Liberation Mono",
    "Ubuntu Mono",
    "Courier New",
];

/// Stores `preference` as a global and applies it once, before any window
/// exists - `System` falls back to `cx.window_appearance()` since there is no
/// `Window` yet to ask. Registers the bundled fonts with the text system
/// first: `apply` needs them loaded before it can set `font_family`.
pub fn init(preference: ThemePreference, cx: &mut App) {
    let fonts = BUNDLED_FONTS
        .iter()
        .map(|font| Cow::Borrowed(*font))
        .collect();
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        log::warn!("failed to register the bundled fonts: {error}");
    }
    cx.set_global(preference);
    apply(preference, None, cx);
}

/// Re-applies the stored preference against `window` and, for `System`, wires
/// a live observer so a mid-session OS appearance change is picked up without
/// a restart. A no-op subscription for a fixed `Light`/`Dark` preference.
///
/// Falls back to `System` when no preference global is set - a test that
/// opens a window directly, skipping [`init`], gets the same default
/// [`ThemePreference`] rather than a panic.
pub fn watch_window(window: &mut Window, cx: &mut App) {
    let preference = cx
        .try_global::<ThemePreference>()
        .copied()
        .unwrap_or_default();
    apply(preference, Some(window), cx);
    if preference == ThemePreference::System {
        window
            .observe_window_appearance(|window, cx| {
                Theme::sync_system_appearance(Some(window), cx);
                apply_fonts(cx);
                crate::ui::accent::refresh(cx);
            })
            .detach();
    }
}

fn apply(preference: ThemePreference, window: Option<&mut Window>, cx: &mut App) {
    match preference {
        ThemePreference::Light => Theme::change(ThemeMode::Light, window, cx),
        ThemePreference::Dark => Theme::change(ThemeMode::Dark, window, cx),
        ThemePreference::System => Theme::sync_system_appearance(window, cx),
    }
    apply_fonts(cx);
    // The system accent colour resolves differently in light and dark mode.
    crate::ui::accent::refresh(cx);
}

/// Sets the frame and code roles' families on the global theme. Called after every
/// `Theme::change`/`sync_system_appearance`, since those calls are gpui-
/// component's own theme-mode reset and would otherwise revert `font_family`/
/// `mono_font_family` to its built-in defaults on the next appearance flip.
fn apply_fonts(cx: &mut App) {
    let mono = first_installed_mono_font(cx);
    let theme = cx.global_mut::<Theme>();
    theme.font_family = FRAME_FAMILY.into();
    theme.mono_font_family = mono;
}

/// `Monaco` when installed, else the first installed alternate, else
/// whatever gpui-component's own platform-default probe already resolved to
/// (its own `.SystemUIFont` last resort) - never a name that isn't actually
/// on the machine, since GPUI panics laying out the first line in a family
/// it cannot find.
fn first_installed_mono_font(cx: &App) -> SharedString {
    let installed = cx.text_system().all_font_names();
    MONO_FONT_ALTERNATES
        .iter()
        .find(|candidate| installed.iter().any(|name| name == *candidate))
        .map(|name| SharedString::from(*name))
        .unwrap_or_else(|| cx.global::<Theme>().mono_font_family.clone())
}

#[cfg(test)]
mod tests {
    use super::{FRAME_FAMILY, ThemePreference, init};
    use gpui_kit::TestAppContext;
    use gpui_kit::component::Theme;

    /// Every `ThemePreference` variant ends with the same two font fields
    /// set - the theme-mode dispatch differs, the font assertion afterward
    /// does not, since `apply_fonts` runs unconditionally at the end of
    /// `apply` regardless of which mode branch ran.
    #[gpui_kit::test]
    async fn every_preference_ends_with_the_frame_font_and_a_real_mono_font(
        cx: &mut TestAppContext,
    ) {
        for preference in [
            ThemePreference::Light,
            ThemePreference::Dark,
            ThemePreference::System,
        ] {
            cx.update(|cx| {
                gpui_kit::init(cx);
                init(preference, cx);

                let theme = cx.global::<Theme>();
                assert_eq!(
                    theme.font_family.as_ref(),
                    FRAME_FAMILY,
                    "{preference:?} should set the UI font to the frame role's"
                );
                assert!(
                    !theme.mono_font_family.is_empty(),
                    "{preference:?} should leave a real mono font family, not empty"
                );
            });
        }
    }
}
