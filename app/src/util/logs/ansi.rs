//! ANSI escapes in a log line (#309): the SGR sequences (`ESC[…m`) a
//! container writes to colour and weight its output become [`Span`]s over the
//! line's plain text, and every other escape - cursor moves, erases, window
//! titles, hyperlinks - is dropped, so none shows as text.
//!
//! A line is parsed once, as it arrives; drawing reads the spans. Its plain
//! text is what search, filtering and copying see. Styling carries from one
//! line to the next of the same stream, as a terminal and k9s's log view carry
//! it: a block coloured once on its first line stays coloured until reset.
//! GPUI-free, like the line model; colours become theme colours when drawn.

use std::ops::Range;

/// A colour an SGR sequence names: one of the 16 standard and bright colours
/// (0-7, then 8-15), which the theme decides, or an exact one from the
/// 256-colour cube and grey ramp or a truecolor sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Colour {
    Base(u8),
    Rgb(u8, u8, u8),
}

/// The SGR state at some point in a line. `Default` is the terminal's reset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub foreground: Option<Colour>,
    pub background: Option<Colour>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Style {
    /// Whether this is the reset state, which draws as plain text.
    pub fn is_plain(&self) -> bool {
        *self == Self::default()
    }
}

/// A run of the plain text drawn in one style, by byte range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub style: Style,
}

/// A line with its escapes taken out: the text, the styled runs of it (none
/// for text that is plain throughout), and the style in force at its end,
/// which the stream's next line starts in.
#[derive(Debug, PartialEq, Eq)]
pub struct Parsed {
    pub text: String,
    pub spans: Vec<Span>,
    pub end: Style,
}

/// Parses `raw`, starting in `carried`, the style the stream's last line
/// ended in. A line with no escape is returned as it came, without a copy.
pub fn parse(raw: String, carried: Style) -> Parsed {
    if !raw.contains('\x1b') {
        let spans = if carried.is_plain() || raw.is_empty() {
            Vec::new()
        } else {
            vec![Span {
                range: 0..raw.len(),
                style: carried,
            }]
        };
        return Parsed {
            text: raw,
            spans,
            end: carried,
        };
    }
    let mut line = Line {
        text: String::with_capacity(raw.len()),
        spans: Vec::new(),
        style: carried,
        start: 0,
    };
    vte::Parser::new().advance(&mut line, raw.as_bytes());
    line.close_span();
    Parsed {
        text: line.text,
        spans: line.spans,
        end: line.style,
    }
}

/// The parser's sink: the text printed so far, the spans closed so far, and
/// the style of the run still open, which began at `start`.
struct Line {
    text: String,
    spans: Vec<Span>,
    style: Style,
    start: usize,
}

impl Line {
    /// Ends the open run at the text's end, keeping it if it's styled and not
    /// empty, and starts the next one there.
    fn close_span(&mut self) {
        let end = self.text.len();
        if end > self.start && !self.style.is_plain() {
            self.spans.push(Span {
                range: self.start..end,
                style: self.style,
            });
        }
        self.start = end;
    }

    /// Applies one SGR sequence's parameters, each group a parameter and its
    /// colon-separated subparameters.
    fn select_graphic_rendition(&mut self, groups: &[&[u16]]) {
        let mut style = self.style;
        if groups.is_empty() {
            style = Style::default();
        }
        let mut ix = 0;
        while let Some(group) = groups.get(ix) {
            let code = group.first().copied().unwrap_or(0);
            match code {
                0 => style = Style::default(),
                1 => style.bold = true,
                2 => style.dim = true,
                3 => style.italic = true,
                4 => style.underline = true,
                22 => (style.bold, style.dim) = (false, false),
                23 => style.italic = false,
                24 => style.underline = false,
                30..=37 => style.foreground = Some(Colour::Base((code - 30) as u8)),
                39 => style.foreground = None,
                40..=47 => style.background = Some(Colour::Base((code - 40) as u8)),
                49 => style.background = None,
                90..=97 => style.foreground = Some(Colour::Base((code - 90 + 8) as u8)),
                100..=107 => style.background = Some(Colour::Base((code - 100 + 8) as u8)),
                38 | 48 => {
                    // `38:5:n` carries its colour in the group; `38;5;n` in
                    // the groups after it, which it then consumes.
                    let (colour, consumed) = if group.len() > 1 {
                        (extended(&group[1..]), 0)
                    } else {
                        let following: Vec<u16> = groups[ix + 1..]
                            .iter()
                            .map(|group| group.first().copied().unwrap_or(0))
                            .collect();
                        extended_following(&following)
                    };
                    if let Some(colour) = colour {
                        if code == 38 {
                            style.foreground = Some(colour);
                        } else {
                            style.background = Some(colour);
                        }
                    }
                    ix += consumed;
                }
                _ => {}
            }
            ix += 1;
        }
        if style != self.style {
            self.close_span();
            self.style = style;
        }
    }
}

/// An extended colour from a colon group's subparameters: `5:n`, `2:r:g:b`,
/// or `2:<colour space>:r:g:b`.
fn extended(values: &[u16]) -> Option<Colour> {
    match values {
        [5, n, ..] => palette(*n),
        [2, _, r, g, b, ..] | [2, r, g, b] => rgb(*r, *g, *b),
        _ => None,
    }
}

/// An extended colour from the parameters after a `38` or `48`, and how many
/// of them it used: `5;n` uses two, `2;r;g;b` four. A truncated or unknown
/// form sets nothing and uses the rest of the sequence, as tview (k9s) does:
/// there's no telling where its parameters end.
fn extended_following(values: &[u16]) -> (Option<Colour>, usize) {
    match values {
        [5, n, ..] => (palette(*n), 2),
        [2, r, g, b, ..] => (rgb(*r, *g, *b), 4),
        _ => (None, values.len()),
    }
}

fn rgb(r: u16, g: u16, b: u16) -> Option<Colour> {
    Some(Colour::Rgb(
        u8::try_from(r).ok()?,
        u8::try_from(g).ok()?,
        u8::try_from(b).ok()?,
    ))
}

/// Colour `n` of the 256-colour palette: the 16 base colours, the 6×6×6 cube
/// on xterm's levels, then the 24-step grey ramp.
fn palette(n: u16) -> Option<Colour> {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let n = u8::try_from(n).ok()?;
    Some(match n {
        0..=15 => Colour::Base(n),
        16..=231 => {
            let cube = n - 16;
            Colour::Rgb(
                LEVELS[usize::from(cube / 36)],
                LEVELS[usize::from(cube / 6 % 6)],
                LEVELS[usize::from(cube % 6)],
            )
        }
        232..=255 => {
            let grey = 8 + 10 * (n - 232);
            Colour::Rgb(grey, grey, grey)
        }
    })
}

impl vte::Perform for Line {
    fn print(&mut self, c: char) {
        self.text.push(c);
    }

    /// Control characters: a tab stays, as it would in a line without
    /// escapes; the rest - carriage returns, bells, backspaces - go.
    fn execute(&mut self, byte: u8) {
        if byte == b'\t' {
            self.text.push('\t');
        }
    }

    /// SGR is the only control sequence kept, as styling. Private and
    /// intermediate forms (`ESC[?25l`, `ESC[>4m`) and every other final byte
    /// are dropped.
    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        if action == 'm' && intermediates.is_empty() && !ignore {
            let groups: Vec<&[u16]> = params.iter().collect();
            self.select_graphic_rendition(&groups);
        }
    }
}

#[cfg(test)]
mod tests;
