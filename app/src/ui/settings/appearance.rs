//! The Settings window's Appearance section (#139): Text Size, and the theme
//! (System, Light or Dark - `UiConfig.theme`). The theme buttons dispatch the
//! same commands as the palette, the View menu and the status bar's switcher
//! (`ui::theme`), so the theme changes live in every window and is saved. The
//! buttons are tab stops; Enter or Space presses them.

use crate::config::ui::Theme as ThemePreference;
use crate::ui::theme::{FollowSystemTheme, UseDarkTheme, UseLightTheme};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// The element id of the theme button for `preference`.
pub(super) fn theme_button_id(preference: ThemePreference) -> &'static str {
    match preference {
        ThemePreference::System => "settings-theme-system",
        ThemePreference::Light => "settings-theme-light",
        ThemePreference::Dark => "settings-theme-dark",
    }
}

/// The theme row: label, then one button per choice, the current one marked.
fn theme_row(cx: &App) -> impl IntoElement {
    let current = crate::ui::theme::current(cx);
    let choice = |preference: ThemePreference, label: &'static str, action: Box<dyn Action>| {
        Button::new(theme_button_id(preference))
            .label(label)
            .xsmall()
            .when(preference == current, |button| button.primary())
            .when(preference != current, |button| button.ghost())
            .on_click(move |_event, window, cx| window.dispatch_action(action.boxed_clone(), cx))
    };
    div()
        .flex()
        .items_center()
        .gap(crate::ui::space::spacing(cx).control_gap)
        .child(div().w(rems(7.5)).child("Theme"))
        .child(choice(
            ThemePreference::System,
            "System",
            Box::new(FollowSystemTheme),
        ))
        .child(choice(
            ThemePreference::Light,
            "Light",
            Box::new(UseLightTheme),
        ))
        .child(choice(
            ThemePreference::Dark,
            "Dark",
            Box::new(UseDarkTheme),
        ))
}

/// The section: a Text card, then a Theme card.
pub(super) fn section(window: &mut Window, cx: &mut App) -> impl IntoElement {
    use super::card::{body, card, hint};
    let text = card("Text", cx)
        .child(super::text_size::row(window, cx))
        .child(hint(
            "Sizes every window's text, and the spacing around it.",
            cx,
        ));
    let theme = card("Theme", cx).child(theme_row(cx)).child(hint(
        "System follows your computer's light or dark appearance.",
        cx,
    ));
    body(cx).child(text).child(theme)
}
