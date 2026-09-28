//! Applies the user's `UiConfig::theme` preference to gpui-component's global
//! `Theme`, once at startup and again on every live OS appearance change when
//! the preference is `System` - so light/dark mode is a day-one concern
//! rather than something bolted on after the picker and main window exist.

use crate::config::ui::Theme as ThemePreference;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;

impl Global for ThemePreference {}

/// Stores `preference` as a global and applies it once, before any window
/// exists - `System` falls back to `cx.window_appearance()` since there is no
/// `Window` yet to ask.
pub fn init(preference: ThemePreference, cx: &mut App) {
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
}
