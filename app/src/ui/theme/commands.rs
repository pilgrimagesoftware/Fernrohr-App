//! The theme preference's palette commands (`toolbar-layout-with-gpui-kit` 2.1):
//! one per choice, in the View menu, each making that theme current in every
//! window. The status bar's switcher dispatches the same actions.

use crate::command::{Command, CommandRegistry, MenuSlot};
use crate::config::ui::Theme as ThemePreference;
use gpui_kit::{App, actions};

actions!(theme, [FollowSystemTheme, UseLightTheme, UseDarkTheme]);

/// The three choices, global, with no default key.
pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    for (id, title, action) in [
        (
            "theme.system",
            "Theme: Follow System",
            Box::new(FollowSystemTheme) as Box<dyn gpui_kit::Action>,
        ),
        ("theme.light", "Theme: Light", Box::new(UseLightTheme)),
        ("theme.dark", "Theme: Dark", Box::new(UseDarkTheme)),
    ] {
        registry.register(Command {
            id,
            title,
            default_binding: "",
            context: None,
            action,
            menu: Some(MenuSlot::View),
        });
    }
}

/// App-wide handlers: the theme isn't any one window's.
pub(crate) fn register_handlers(cx: &mut App) {
    cx.on_action(|_: &FollowSystemTheme, cx: &mut App| super::set(ThemePreference::System, cx));
    cx.on_action(|_: &UseLightTheme, cx: &mut App| super::set(ThemePreference::Light, cx));
    cx.on_action(|_: &UseDarkTheme, cx: &mut App| super::set(ThemePreference::Dark, cx));
}
