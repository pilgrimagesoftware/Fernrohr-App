//! The app's visual language as named tokens: one accent, three surface
//! levels, status tones, and a contrast floor (`visual-definition`).
//!
//! Views ask for these rather than raw theme fields, so the next view is
//! consistent by default and a theme change lands here. Everything is derived
//! from the current gpui-component theme (its background, `success`/`warning`/
//! `danger`) and the system accent ([`crate::ui::accent`]), then nudged until
//! it meets the contrast floor: text 4.5:1 on every surface, accent and status
//! colours 3:1 on the background.

use gpui_kit::component::{ActiveTheme as _, Theme};
use gpui_kit::*;

/// WCAG AA for body text.
pub const TEXT_CONTRAST: f32 = 4.5;
/// WCAG AA for non-text marks: the accent, status colours.
pub const MARK_CONTRAST: f32 = 3.0;

/// How far a raised surface's lightness steps from the background. A card
/// steps half again as far; a stripe half as far.
const SURFACE_STEP: f32 = 0.035;
/// The accent's opacity when it tints a selected row.
const SELECTION_ALPHA: f32 = 0.22;
/// The accent's opacity when it tints a hovered row.
const HOVER_ALPHA: f32 = 0.10;

/// A status tone, mapped to colour only here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Good,
    Warning,
    Bad,
    Neutral,
}

/// The accent: the system accent (else the theme's blue), made to meet 3:1
/// against the background.
pub fn accent(cx: &App) -> Hsla {
    let theme = cx.theme();
    meet(
        crate::ui::accent::focus_accent(cx),
        theme.background,
        MARK_CONTRAST,
    )
}

/// The accent as a tint over the background, for a selected row.
pub fn accent_subtle(cx: &App) -> Hsla {
    accent(cx).opacity(SELECTION_ALPHA)
}

/// Panel chrome: a panel's heading and hint bar, table and section headers.
pub fn surface_raised(cx: &App) -> Hsla {
    step(cx.theme(), SURFACE_STEP)
}

/// A card inside a detail view.
pub fn surface_card(cx: &App) -> Hsla {
    step(cx.theme(), SURFACE_STEP * 1.5)
}

/// Every other structured row in a detail view.
pub fn stripe(cx: &App) -> Hsla {
    step(cx.theme(), SURFACE_STEP / 2.)
}

/// A tone's colour, made to meet 3:1 against the background.
pub fn status(tone: Tone, cx: &App) -> Hsla {
    let theme = cx.theme();
    let colour = match tone {
        Tone::Good => theme.success,
        Tone::Warning => theme.warning,
        Tone::Bad => theme.danger,
        Tone::Neutral => return theme.muted_foreground,
    };
    meet(colour, theme.background, MARK_CONTRAST)
}

/// The background, stepped in lightness away from the text: lighter in dark
/// mode, darker in light mode.
fn step(theme: &Theme, amount: f32) -> Hsla {
    let mut surface = theme.background;
    surface.l = if theme.mode.is_dark() {
        (surface.l + amount).min(1.)
    } else {
        (surface.l - amount).max(0.)
    };
    surface
}

/// `colour` moved in lightness - away from `background` - until it meets
/// `ratio` against it, in small steps so a system colour keeps its hue and as
/// much of its lightness as contrast allows.
pub fn meet(colour: Hsla, background: Hsla, ratio: f32) -> Hsla {
    let darken = luminance(background) > 0.5;
    let mut colour = colour;
    for _ in 0..40 {
        if contrast(colour, background) >= ratio {
            break;
        }
        colour.l = if darken {
            (colour.l - 0.025).max(0.)
        } else {
            (colour.l + 0.025).min(1.)
        };
    }
    colour
}

/// The WCAG contrast ratio between two colours, 1 to 21. A translucent colour
/// is composited over `b` first - the way it's drawn.
pub fn contrast(a: Hsla, b: Hsla) -> f32 {
    let a = over(a, b);
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// `top` composited over an opaque `bottom`.
pub fn over(top: Hsla, bottom: Hsla) -> Hsla {
    if top.a >= 1. {
        return top;
    }
    let (t, b) = (top.to_rgb(), bottom.to_rgb());
    let mix = |t: f32, b: f32| t * top.a + b * (1. - top.a);
    Rgba {
        r: mix(t.r, b.r),
        g: mix(t.g, b.g),
        b: mix(t.b, b.b),
        a: 1.,
    }
    .into()
}

/// WCAG relative luminance of an opaque colour.
fn luminance(colour: Hsla) -> f32 {
    let rgb = colour.to_rgb();
    let channel = |c: f32| {
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(rgb.r) + 0.7152 * channel(rgb.g) + 0.0722 * channel(rgb.b)
}

/// Sets the theme colours gpui-component's own widgets draw with, so the tab
/// strip, buttons, lists and tables speak the same accent. Called after every
/// theme change (from [`crate::ui::accent::refresh`], which runs after each),
/// since `Theme::change` resets them.
///
/// Each is set twice: on the theme's colours, which app code reads, and on its
/// resolved `tokens`, which gpui-component's widgets read (`tokens.primary`,
/// `tokens.table_active`...). `Theme::change` rebuilds the tokens from the
/// colours once, so a colour set afterwards reaches no widget on its own - and
/// derived tokens such as `button_primary` need setting too.
pub fn apply_overrides(cx: &mut App) {
    let accent = accent(cx);
    let selected = accent_subtle(cx);
    let hover = accent.opacity(HOVER_ALPHA);
    let is_dark = cx.theme().mode.is_dark();
    // Pressed and hovered primaries step toward the text, like the library's.
    let shade = |amount: f32| {
        let mut shaded = accent;
        shaded.l = if is_dark {
            (shaded.l + amount).min(1.)
        } else {
            (shaded.l - amount).max(0.)
        };
        shaded
    };
    let (primary_hover, primary_active) = (shade(0.05), shade(0.10));
    let on_accent = readable_on(accent);
    // Muted text is text: it has to meet 4.5:1 on every surface too, which the
    // library's default grey misses in light mode on the card and on a
    // selected row. The card is the plain surface furthest from the background
    // and the selected row the darkest tint; `meet` only moves the text further
    // from the background, so meeting both keeps the rest met.
    let background = cx.theme().background;
    let muted = [surface_card(cx), over(selected, background)]
        .into_iter()
        .fold(cx.theme().muted_foreground, |muted, surface| {
            meet(muted, surface, TEXT_CONTRAST)
        });
    let foreground = cx.theme().foreground;
    let raised = surface_raised(cx);

    let theme = cx.global_mut::<Theme>();
    macro_rules! set {
        ($($field:ident = $value:expr),+ $(,)?) => {
            $(
                theme.$field = $value;
                theme.tokens.$field = ($value).into();
            )+
        };
    }
    set! {
        muted_foreground = muted,
        primary = accent,
        primary_hover = primary_hover,
        primary_active = primary_active,
        primary_foreground = on_accent,
        button_primary = accent,
        button_primary_hover = primary_hover,
        button_primary_active = primary_active,
        button_primary_foreground = on_accent,
        ring = accent,
        link = accent,
        link_hover = primary_hover,
        link_active = primary_active,
        list_active = selected,
        list_active_border = accent,
        table_active = selected,
        table_active_border = accent,
        table_hover = hover,
        table_head = raised,
        accent = hover,
        accent_foreground = foreground,
    }
}

/// Black or white, whichever reads better on `fill`.
fn readable_on(fill: Hsla) -> Hsla {
    let (white, black) = (hsla(0., 0., 1., 1.), hsla(0., 0., 0., 1.));
    if contrast(white, fill) >= contrast(black, fill) {
        white
    } else {
        black
    }
}

#[cfg(test)]
mod tests;
