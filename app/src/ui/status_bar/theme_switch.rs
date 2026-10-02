//! The theme switcher at the status bar's far end (`toolbar-layout-with-gpui-kit`
//! 2.1): a button showing the current choice, whose menu lists System, Light and
//! Dark with the current one checked. Each item dispatches the same action as its
//! palette command, so the menu and the palette take one path.

use crate::config::ui::Theme as ThemePreference;
use crate::ui::theme::{self, FollowSystemTheme, UseDarkTheme, UseLightTheme};
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::*;

fn icon(preference: ThemePreference) -> IconName {
    match preference {
        ThemePreference::System => IconName::Monitor,
        ThemePreference::Light => IconName::Sun,
        ThemePreference::Dark => IconName::Moon,
    }
}

pub(super) fn render_theme_switch(cx: &App) -> impl IntoElement {
    let current = theme::current(cx);
    Button::new("status-theme-switch")
        .icon(icon(current))
        .xsmall()
        .ghost()
        .tooltip("Theme")
        .dropdown_menu(move |menu, _window, _cx| {
            let item = |label: &'static str, choice: ThemePreference, action: Box<dyn Action>| {
                PopupMenuItem::new(label)
                    .checked(current == choice)
                    .on_click(move |_event, window, cx| {
                        window.dispatch_action(action.boxed_clone(), cx)
                    })
            };
            menu.item(item(
                "System",
                ThemePreference::System,
                Box::new(FollowSystemTheme),
            ))
            .item(item(
                "Light",
                ThemePreference::Light,
                Box::new(UseLightTheme),
            ))
            .item(item("Dark", ThemePreference::Dark, Box::new(UseDarkTheme)))
        })
}
