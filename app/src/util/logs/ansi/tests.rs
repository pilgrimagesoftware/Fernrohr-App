//! #309: every SGR form, carried and reset style, malformed and truncated
//! sequences, and every other escape stripped.

use super::{Colour, Span, Style, parse};

const ESC: &str = "\x1b";

/// `raw` parsed from the reset state: its text and spans.
fn read(raw: &str) -> (String, Vec<Span>) {
    let parsed = parse(raw.to_string(), Style::default());
    (parsed.text, parsed.spans)
}

fn fg(colour: Colour) -> Style {
    Style {
        foreground: Some(colour),
        ..Style::default()
    }
}

fn span(range: std::ops::Range<usize>, style: Style) -> Span {
    Span { range, style }
}

#[test]
fn a_plain_line_has_no_spans_and_is_not_copied() {
    let raw = "level=info msg=ready".to_string();
    let pointer = raw.as_ptr();
    let parsed = parse(raw, Style::default());
    assert_eq!(parsed.text, "level=info msg=ready");
    assert!(parsed.spans.is_empty());
    assert_eq!(parsed.text.as_ptr(), pointer, "returned as it came");
}

#[test]
fn standard_and_bright_foregrounds_and_backgrounds() {
    let (text, spans) = read(&format!(
        "{ESC}[31mred{ESC}[0m {ESC}[92mlime{ESC}[39m {ESC}[44mon blue{ESC}[49m {ESC}[103mon yellow{ESC}[m"
    ));
    assert_eq!(text, "red lime on blue on yellow");
    let on = |colour| Style {
        background: Some(colour),
        ..Style::default()
    };
    assert_eq!(
        spans,
        [
            span(0..3, fg(Colour::Base(1))),
            span(4..8, fg(Colour::Base(10))),
            span(9..16, on(Colour::Base(4))),
            span(17..26, on(Colour::Base(11))),
        ]
    );
}

#[test]
fn bold_dim_italic_underline_and_their_resets() {
    let (text, spans) = read(&format!(
        "{ESC}[1mb{ESC}[2md{ESC}[22m{ESC}[3mi{ESC}[23m{ESC}[4mu{ESC}[24mx"
    ));
    assert_eq!(text, "bdiux");
    let bold = Style {
        bold: true,
        ..Style::default()
    };
    let bold_dim = Style { dim: true, ..bold };
    let italic = Style {
        italic: true,
        ..Style::default()
    };
    let underline = Style {
        underline: true,
        ..Style::default()
    };
    assert_eq!(
        spans,
        [
            span(0..1, bold),
            span(1..2, bold_dim),
            span(2..3, italic),
            span(3..4, underline),
        ]
    );
}

#[test]
fn several_attributes_in_one_sequence() {
    let (text, spans) = read(&format!("{ESC}[1;4;31mwarn{ESC}[0m"));
    assert_eq!(text, "warn");
    let style = Style {
        foreground: Some(Colour::Base(1)),
        bold: true,
        underline: true,
        ..Style::default()
    };
    assert_eq!(spans, [span(0..4, style)]);
}

#[test]
fn the_256_colour_palette_in_both_separators() {
    for raw in [
        format!("{ESC}[38;5;9ma{ESC}[38;5;196mb{ESC}[38;5;244mc"),
        format!("{ESC}[38:5:9ma{ESC}[38:5:196mb{ESC}[38:5:244mc"),
    ] {
        let (text, spans) = read(&raw);
        assert_eq!(text, "abc");
        assert_eq!(
            spans,
            [
                span(0..1, fg(Colour::Base(9))),
                span(1..2, fg(Colour::Rgb(255, 0, 0))),
                span(2..3, fg(Colour::Rgb(128, 128, 128))),
            ],
            "{raw:?}"
        );
    }
    let (_, spans) = read(&format!("{ESC}[48;5;21mx"));
    let on_blue = Style {
        background: Some(Colour::Rgb(0, 0, 255)),
        ..Style::default()
    };
    assert_eq!(spans, [span(0..1, on_blue)]);
}

#[test]
fn truecolor_in_both_separators() {
    for raw in [
        format!("{ESC}[38;2;10;20;30mx"),
        format!("{ESC}[38:2:10:20:30mx"),
        format!("{ESC}[38:2::10:20:30mx"),
    ] {
        let (text, spans) = read(&raw);
        assert_eq!(text, "x");
        assert_eq!(spans, [span(0..1, fg(Colour::Rgb(10, 20, 30)))], "{raw:?}");
    }
}

#[test]
fn an_extended_colour_consumes_its_parameters_and_no_more() {
    let (_, spans) = read(&format!("{ESC}[38;2;10;20;30;1mx"));
    let style = Style {
        foreground: Some(Colour::Rgb(10, 20, 30)),
        bold: true,
        ..Style::default()
    };
    assert_eq!(spans, [span(0..1, style)]);
}

#[test]
fn malformed_colours_set_nothing() {
    for raw in [
        format!("{ESC}[38;5mx"),
        format!("{ESC}[38;2;1;2mx"),
        format!("{ESC}[38;5;300mx"),
        format!("{ESC}[38;2;256;0;0mx"),
        format!("{ESC}[38;9;1mx"),
        format!("{ESC}[38mx"),
    ] {
        let (text, spans) = read(&raw);
        assert_eq!(text, "x", "{raw:?}");
        assert!(spans.is_empty(), "{raw:?}: {spans:?}");
    }
}

#[test]
fn a_truncated_sequence_at_the_end_shows_nothing() {
    for raw in [
        format!("done{ESC}"),
        format!("done{ESC}["),
        format!("done{ESC}[3"),
        format!("done{ESC}[38;5;"),
        format!("done{ESC}]0;title"),
    ] {
        assert_eq!(read(&raw).0, "done", "{raw:?}");
    }
}

#[test]
fn every_other_escape_is_stripped() {
    let (text, spans) = read(&format!(
        "{ESC}[2K{ESC}[1G{ESC}[?25lstep {ESC}[3A1{ESC}]0;title\x07 {ESC}]8;;https://example.com\x1b\\link{ESC}]8;;\x1b\\ {ESC}(Bdone{ESC}[?25h"
    ));
    assert_eq!(text, "step 1 link done");
    assert!(spans.is_empty(), "{spans:?}");
}

#[test]
fn control_characters_go_but_tabs_stay() {
    let (text, _) = read(&format!("a\tb\r{ESC}[31mc\x07\x08"));
    assert_eq!(text, "a\tbc");
}

#[test]
fn multibyte_text_keeps_byte_ranges() {
    let (text, spans) = read(&format!("✓ {ESC}[32mgrün{ESC}[0m"));
    assert_eq!(text, "✓ grün");
    assert_eq!(spans, [span(4..9, fg(Colour::Base(2)))]);
    assert_eq!(&text[spans[0].range.clone()], "grün");
}

#[test]
fn style_carries_into_the_next_line_until_reset() {
    let first = parse(format!("{ESC}[31mpanic: boom"), Style::default());
    assert_eq!(first.end, fg(Colour::Base(1)));
    let second = parse("  at main.go:12".to_string(), first.end);
    assert_eq!(second.spans, [span(0..15, fg(Colour::Base(1)))]);
    let third = parse(format!("  at lib.go:4{ESC}[0m"), second.end);
    assert_eq!(third.spans, [span(0..13, fg(Colour::Base(1)))]);
    assert!(third.end.is_plain(), "reset at its end");
}
