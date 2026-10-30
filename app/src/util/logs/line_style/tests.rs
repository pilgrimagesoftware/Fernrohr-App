//! #309: the base colours stay readable on the panel in either mode, and a
//! style maps to the highlight that draws it.

use super::AnsiPalette;
use crate::util::logs::ansi::{Colour, Span, Style};
use gpui_kit::{FontStyle, FontWeight, hsla};

fn palettes() -> [(AnsiPalette, gpui_kit::Hsla); 2] {
    let dark = hsla(0., 0., 0.1, 1.);
    let light = hsla(0., 0., 0.98, 1.);
    [
        (AnsiPalette::new(dark, true), dark),
        (AnsiPalette::new(light, false), light),
    ]
}

#[test]
fn every_base_colour_reads_on_the_background_in_both_modes() {
    for (palette, background) in palettes() {
        for ix in 0..16u8 {
            let colour = palette.foreground(Colour::Base(ix), None);
            let contrast = crate::ui::style::contrast(colour, background);
            assert!(
                contrast >= crate::ui::style::TEXT_CONTRAST - 0.01,
                "colour {ix} on {background:?}: {contrast}"
            );
        }
    }
}

#[test]
fn an_unreadable_exact_colour_is_made_readable() {
    for (palette, background) in palettes() {
        // Near the background either way: black on dark, white on light.
        for exact in [Colour::Rgb(0, 0, 0), Colour::Rgb(255, 255, 255)] {
            let colour = palette.foreground(exact, None);
            assert!(
                crate::ui::style::contrast(colour, background)
                    >= crate::ui::style::TEXT_CONTRAST - 0.01
            );
        }
    }
}

#[test]
fn attributes_map_to_the_highlight() {
    let (palette, _) = palettes().into_iter().next().unwrap();
    let style = Style {
        foreground: Some(Colour::Base(1)),
        background: Some(Colour::Base(4)),
        bold: true,
        dim: true,
        italic: true,
        underline: true,
    };
    let highlight = palette.highlight(&style);
    assert_eq!(
        highlight.color,
        Some(palette.foreground(Colour::Base(1), None))
    );
    assert!(highlight.background_color.is_some());
    assert_eq!(highlight.font_weight, Some(FontWeight::BOLD));
    assert_eq!(highlight.font_style, Some(FontStyle::Italic));
    assert!(highlight.underline.is_some());
    assert!(highlight.fade_out.is_some());
    assert_eq!(palette.highlight(&Style::default()), Default::default());
}

#[test]
fn highlights_are_shifted_past_a_tag() {
    let (palette, _) = palettes().into_iter().next().unwrap();
    let spans = [Span {
        range: 2..5,
        style: Style {
            bold: true,
            ..Style::default()
        },
    }];
    let ranges: Vec<_> = palette
        .highlights(&spans, 10)
        .map(|(range, _)| range)
        .collect();
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0], 12..15);
}
