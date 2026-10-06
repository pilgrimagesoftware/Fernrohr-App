//! The shell terminal's look (`embedded-exec-terminal` decision 3): its font,
//! colours and size from the app's theme, so a shell reads like the rest of
//! the app and follows its light/dark mode and text size.
//!
//! The sixteen ANSI colours are the theme's six hues and their light
//! variants. Black and white swap roles between the modes, so text a program
//! draws in "black" or "white" stays readable against the background.

use gpui_kit::App;
use gpui_kit::component::ActiveTheme as _;
use gpui_terminal::{TerminalPalette, TerminalStyle};

/// The terminal's style now: the theme's code font at its current size,
/// in [`palette`].
pub(super) fn terminal_style(cx: &App) -> TerminalStyle {
    let theme = cx.theme();
    TerminalStyle::new(theme.mono_font_family.clone(), theme.mono_font_size).palette(palette(cx))
}

/// The theme's colours as a terminal palette.
pub(super) fn palette(cx: &App) -> TerminalPalette {
    let theme = cx.theme();
    let rgb = TerminalPalette::rgb;
    let dark = theme.mode.is_dark();
    // On a dark background "black" is the muted surface and "white" the
    // text; on a light one, the other way round.
    let (black, white) = if dark {
        (theme.muted, theme.foreground)
    } else {
        (theme.foreground, theme.muted)
    };
    let ansi = [
        black,
        theme.red,
        theme.green,
        theme.yellow,
        theme.blue,
        theme.magenta,
        theme.cyan,
        white,
        theme.muted_foreground,
        theme.red_light,
        theme.green_light,
        theme.yellow_light,
        theme.blue_light,
        theme.magenta_light,
        theme.cyan_light,
        theme.foreground,
    ]
    .map(rgb);
    TerminalPalette {
        foreground: rgb(theme.foreground),
        background: rgb(theme.background),
        ansi,
        cursor: Some(rgb(theme.caret)),
        // The theme's selection is translucent; the terminal paints it opaque,
        // so blend it over the background first.
        selection: Some(rgb(crate::ui::style::over(
            theme.selection,
            theme.background,
        ))),
    }
}
