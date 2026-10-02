//! The text-size preference (`typography` spec, "Configurable text size"):
//! one factor that scales the frame, data and code roles' text together, and
//! the spacing tokens with them (`ui::space`).
//!
//! The size reaches every window through two globals. gpui-component's
//! `Theme::font_size` is the rem every `Root` sets on its window each frame,
//! so every rem-based size (`text_sm`, `text_xs`, `p_2`, ...) follows it, and
//! `mono_font_size` sizes code. [`TextScale`] is what the spacing tokens read.
//! Both are derived from the [`TextSize`] preference and the 100% sizes in
//! `consts`, never from their own current values, so applying twice can't
//! compound.

use crate::config::{self, ui::TextSize, ui::UiConfig};
use crate::consts::{DEFAULT_FONT_SIZE, DEFAULT_MONO_FONT_SIZE};
use crate::ui::space::TextScale;
use gpui_kit::component::Theme;
use gpui_kit::*;
use std::path::PathBuf;

/// The current preference, and the preference file it's saved to. No file
/// means nothing is saved, which is what tests that never call [`init`] get.
struct Preference {
    size: TextSize,
    path: Option<PathBuf>,
}

impl Global for Preference {}

/// Applies `size` at startup and remembers `path` (the preference file,
/// `ui.toml`) so later changes are saved there. Call after `ui::theme::init`,
/// which creates the theme this sizes.
pub fn init(size: TextSize, path: PathBuf, cx: &mut App) {
    cx.set_global(Preference {
        size,
        path: Some(path),
    });
    apply(size, cx);
}

/// The current text size: the one last set, else the default.
// UNWIRED: the text-size commands and the Settings stepper (section 4.2) call
// `current` and `set`.
#[allow(dead_code)]
pub fn current(cx: &App) -> TextSize {
    cx.try_global::<Preference>()
        .map_or(TextSize::DEFAULT, |preference| preference.size)
}

/// Makes `size` the text size: every open window redraws at it, and it's
/// saved to the preference file. A no-op when `size` is already current,
/// so stepping past a bound neither redraws nor writes.
#[allow(dead_code)]
pub fn set(size: TextSize, cx: &mut App) {
    if size == current(cx) {
        return;
    }
    let path = cx
        .try_global::<Preference>()
        .and_then(|preference| preference.path.clone());
    cx.set_global(Preference {
        size,
        path: path.clone(),
    });
    apply(size, cx);
    if let Some(path) = path {
        save(size, &path);
    }
}

/// Sets the theme's font sizes from the current [`TextScale`]. `ui::theme`
/// calls this after every theme-mode change as well, alongside the font
/// families, so a light/dark flip can't reset them.
pub(crate) fn apply_font_sizes(cx: &mut App) {
    let factor = TextScale::current(cx).factor();
    let theme = cx.global_mut::<Theme>();
    theme.font_size = px(DEFAULT_FONT_SIZE * factor);
    theme.mono_font_size = px(DEFAULT_MONO_FONT_SIZE * factor);
}

fn apply(size: TextSize, cx: &mut App) {
    // `set` only queues the redraw of every window; it runs once this update
    // ends, after the font sizes below are in place.
    TextScale::new(size.factor())
        .expect("every TextSize step is a positive factor")
        .set(cx);
    apply_font_sizes(cx);
}

/// Rewrites only `text_size` in the preference file, keeping the rest of it.
fn save(size: TextSize, path: &std::path::Path) {
    let mut ui: UiConfig = config::load(path);
    ui.text_size = size;
    if let Err(error) = config::save(path, &ui) {
        log::warn!(
            "failed to save the text size to {}: {error}",
            path.display()
        );
    }
}

#[cfg(test)]
mod tests;
