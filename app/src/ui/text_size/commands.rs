//! Increase, Decrease and Reset Text Size: global commands in the View menu
//! and the palette, each one step of the [`TextSize`] preference.
//!
//! Reset is `cmd-shift-0`, not the `cmd-0` most apps use: `cmd-0` is already
//! Focus Resources, one of the `cmd-1`/`cmd-2`/`cmd-0` show-panel keys.

use super::{current, set};
use crate::command::{Command, CommandRegistry, MenuSlot};
use crate::config::ui::TextSize;
use gpui_kit::{App, actions};

actions!(view, [IncreaseTextSize, DecreaseTextSize, ResetTextSize]);

pub(crate) const INCREASE_COMMAND_ID: &str = "view.increase_text_size";
pub(crate) const INCREASE_DEFAULT_BINDING: &str = "cmd-=";
pub(crate) const DECREASE_COMMAND_ID: &str = "view.decrease_text_size";
pub(crate) const DECREASE_DEFAULT_BINDING: &str = "cmd--";
pub(crate) const RESET_COMMAND_ID: &str = "view.reset_text_size";
pub(crate) const RESET_DEFAULT_BINDING: &str = "cmd-shift-0";

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    for (id, title, default_binding, action) in [
        (
            INCREASE_COMMAND_ID,
            "Increase Text Size",
            INCREASE_DEFAULT_BINDING,
            Box::new(IncreaseTextSize) as Box<dyn gpui_kit::Action>,
        ),
        (
            DECREASE_COMMAND_ID,
            "Decrease Text Size",
            DECREASE_DEFAULT_BINDING,
            Box::new(DecreaseTextSize),
        ),
        (
            RESET_COMMAND_ID,
            "Reset Text Size",
            RESET_DEFAULT_BINDING,
            Box::new(ResetTextSize),
        ),
    ] {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: None,
            action,
            menu: Some(MenuSlot::View),
        });
    }
}

/// The app-wide handlers. Called once at startup, beside the other windows'.
pub(crate) fn register_handlers(cx: &mut App) {
    cx.on_action(|_: &IncreaseTextSize, cx: &mut App| set(current(cx).increase(), cx));
    cx.on_action(|_: &DecreaseTextSize, cx: &mut App| set(current(cx).decrease(), cx));
    cx.on_action(|_: &ResetTextSize, cx: &mut App| set(TextSize::DEFAULT, cx));
}

#[cfg(test)]
mod tests;
