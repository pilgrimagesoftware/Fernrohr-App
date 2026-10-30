//! How a log line's ANSI styling (#309) draws: its [`Colour`]s as theme
//! colours - the 16 base colours by the theme's mode, each made readable on
//! the panel's background, as the pod tags are - and its spans as the
//! highlights of the copy of the line drawn over its selectable text.

use super::ansi::{Colour, Span, Style};
use gpui_kit::*;

/// Each base colour's hue, black and white aside, as a terminal's palette
/// orders them: red, green, yellow, blue, magenta, cyan.
const HUES: [f32; 6] = [0., 120., 50., 220., 300., 185.];

/// How much a background colour from the base palette shows: enough to read
/// as that colour, with the text over it still in contrast.
const BASE_BACKGROUND_OPACITY: f32 = 0.3;

/// The 16 base colours for the theme now, and the background they're drawn
/// against. Worked out once per frame, not per line.
#[derive(Clone)]
pub(super) struct AnsiPalette {
    base: [Hsla; 16],
    background: Hsla,
}

impl AnsiPalette {
    pub(super) fn new(background: Hsla, dark: bool) -> Self {
        let base = std::array::from_fn(|ix| {
            let bright = ix >= 8;
            let colour = match ix % 8 {
                0 => hsla(0., 0., if bright { 0.5 } else { 0.2 }, 1.),
                7 => hsla(0., 0., if bright { 0.98 } else { 0.8 }, 1.),
                hue => {
                    let lightness = match (dark, bright) {
                        (true, false) => 0.6,
                        (true, true) => 0.72,
                        (false, false) => 0.35,
                        (false, true) => 0.45,
                    };
                    hsla(HUES[hue - 1] / 360., 0.7, lightness, 1.)
                }
            };
            crate::ui::style::meet(colour, background, crate::ui::style::TEXT_CONTRAST)
        });
        Self { base, background }
    }

    fn background(&self, colour: Colour) -> Hsla {
        match colour {
            Colour::Base(ix) => self.base[usize::from(ix % 16)].opacity(BASE_BACKGROUND_OPACITY),
            Colour::Rgb(r, g, b) => rgb_hsla(r, g, b),
        }
    }

    /// A foreground colour, made readable on what's behind it: the span's
    /// own exact background, or the panel's.
    fn foreground(&self, colour: Colour, behind: Option<Colour>) -> Hsla {
        let behind = match behind {
            Some(Colour::Rgb(r, g, b)) => rgb_hsla(r, g, b),
            _ => self.background,
        };
        match colour {
            Colour::Base(ix) => self.base[usize::from(ix % 16)],
            Colour::Rgb(r, g, b) => {
                crate::ui::style::meet(rgb_hsla(r, g, b), behind, crate::ui::style::TEXT_CONTRAST)
            }
        }
    }

    /// `style` as a text highlight.
    pub(super) fn highlight(&self, style: &Style) -> HighlightStyle {
        HighlightStyle {
            color: style
                .foreground
                .map(|colour| self.foreground(colour, style.background)),
            background_color: style.background.map(|colour| self.background(colour)),
            font_weight: style.bold.then_some(FontWeight::BOLD),
            font_style: style.italic.then_some(FontStyle::Italic),
            underline: style.underline.then(|| UnderlineStyle {
                thickness: px(1.),
                ..Default::default()
            }),
            fade_out: style.dim.then_some(0.4),
            ..Default::default()
        }
    }

    /// The highlights for `spans` of a line whose own text starts `offset`
    /// bytes into the string drawn.
    pub(super) fn highlights(
        &self,
        spans: &[Span],
        offset: usize,
    ) -> impl Iterator<Item = (std::ops::Range<usize>, HighlightStyle)> {
        spans.iter().map(move |span| {
            (
                span.range.start + offset..span.range.end + offset,
                self.highlight(&span.style),
            )
        })
    }
}

fn rgb_hsla(r: u8, g: u8, b: u8) -> Hsla {
    Rgba {
        r: f32::from(r) / 255.,
        g: f32::from(g) / 255.,
        b: f32::from(b) / 255.,
        a: 1.,
    }
    .into()
}

#[cfg(test)]
mod tests;
