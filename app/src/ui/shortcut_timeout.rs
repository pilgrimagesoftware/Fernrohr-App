//! The Shortcut timeout preference (`pending-chord-indicator`): how long a
//! chord whose keys so far are also a whole binding waits for its next key
//! before the shorter binding runs. The status bar's chord indicator reads it
//! each time such a chord starts, so a change applies to the next one at once.
//!
//! Increase, Decrease and Reset are palette commands with no default key -
//! a rarely changed setting needs no shortcut - and the Settings window's
//! stepper dispatches them, so the palette and the stepper take one path.

use crate::command::{Command, CommandRegistry};
use crate::config::{self, ui::ShortcutTimeout, ui::UiConfig};
use gpui_kit::{App, Global, actions};
use std::path::{Path, PathBuf};

actions!(
    settings,
    [
        IncreaseShortcutTimeout,
        DecreaseShortcutTimeout,
        ResetShortcutTimeout
    ]
);

/// The current preference, and the file it's saved to - none in tests that
/// never call [`init`], so nothing is written.
struct Preference {
    timeout: ShortcutTimeout,
    path: Option<PathBuf>,
}

impl Global for Preference {}

/// Remembers the stored timeout and the file (`ui.toml`) it came from.
pub fn init(timeout: ShortcutTimeout, path: PathBuf, cx: &mut App) {
    cx.set_global(Preference {
        timeout,
        path: Some(path),
    });
}

/// The current timeout: the one last set, else the default.
pub fn current(cx: &App) -> ShortcutTimeout {
    cx.try_global::<Preference>()
        .map_or(ShortcutTimeout::DEFAULT, |preference| preference.timeout)
}

/// Makes `timeout` the preference and saves it. A no-op when it's already
/// current, so stepping past a bound writes nothing.
pub fn set(timeout: ShortcutTimeout, cx: &mut App) {
    if timeout == current(cx) {
        return;
    }
    let path = cx
        .try_global::<Preference>()
        .and_then(|preference| preference.path.clone());
    cx.set_global(Preference {
        timeout,
        path: path.clone(),
    });
    if let Some(path) = path {
        save(timeout, &path);
    }
}

/// Rewrites only `shortcut_timeout_secs` in the preference file.
fn save(timeout: ShortcutTimeout, path: &Path) {
    let mut ui: UiConfig = config::load(path);
    ui.shortcut_timeout_secs = timeout;
    if let Err(error) = config::save(path, &ui) {
        log::warn!(
            "failed to save the shortcut timeout to {}: {error}",
            path.display()
        );
    }
}

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    for (id, title, action) in [
        (
            "settings.increase_shortcut_timeout",
            "Increase Shortcut Timeout",
            Box::new(IncreaseShortcutTimeout) as Box<dyn gpui_kit::Action>,
        ),
        (
            "settings.decrease_shortcut_timeout",
            "Decrease Shortcut Timeout",
            Box::new(DecreaseShortcutTimeout),
        ),
        (
            "settings.reset_shortcut_timeout",
            "Reset Shortcut Timeout",
            Box::new(ResetShortcutTimeout),
        ),
    ] {
        registry.register(Command {
            id,
            title,
            default_binding: "",
            context: None,
            action,
            menu: None,
        });
    }
}

/// The app-wide handlers. Called once at startup, beside the other windows'.
pub(crate) fn register_handlers(cx: &mut App) {
    cx.on_action(|_: &IncreaseShortcutTimeout, cx: &mut App| set(current(cx).increase(), cx));
    cx.on_action(|_: &DecreaseShortcutTimeout, cx: &mut App| set(current(cx).decrease(), cx));
    cx.on_action(|_: &ResetShortcutTimeout, cx: &mut App| set(ShortcutTimeout::DEFAULT, cx));
}
