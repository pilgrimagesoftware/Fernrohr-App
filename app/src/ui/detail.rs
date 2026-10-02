//! The pieces every detail view draws its structured fields with: a labelled
//! row, and the value shapes a row can hold - chips, colored badges, plain
//! lines, key/value pairs, section headings and event cards.
//!
//! Shared by pod detail and the generic object viewer (`resource-links`
//! 5.1), so the two read as one application. Each panel keeps its own field
//! model; this module only draws.

use crate::k8s::resource::events::EventSummary;
use crate::k8s::resource::secret_value::{Reveal, RevealError};
use crate::ui::style::{self, Tone};
use crate::ui::typography::TypeRole as _;
use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

mod collapsible;
pub use collapsible::Collapsible;

/// A status's color, decided by the status rather than looked up at render
/// time - so "is this good" is testable without a theme, and the renderer only
/// maps tone to a color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeTone {
    /// The condition holds, or the thing is healthy.
    Good,
    /// Under way - waiting or pending - and not a problem yet.
    Info,
    /// Something to notice - not necessarily a failure.
    Warning,
    /// A failure: the thing won't work as it is.
    Bad,
    /// The cluster did not say either way.
    Unknown,
}

impl From<BadgeTone> for Tone {
    fn from(tone: BadgeTone) -> Self {
        match tone {
            BadgeTone::Good => Tone::Good,
            BadgeTone::Info => Tone::Info,
            BadgeTone::Warning => Tone::Warning,
            BadgeTone::Bad => Tone::Bad,
            BadgeTone::Unknown => Tone::Neutral,
        }
    }
}

/// A tone's colour: `ui::style`'s one status table.
pub fn tone_color(tone: BadgeTone, cx: &App) -> Hsla {
    style::status(tone.into(), cx)
}

/// One field row: a fixed-width label, quieter than its value (muted and a
/// size down), and the value beside it. Both are data text; a button inside
/// the value sets the frame role back itself.
pub fn row(label: impl Into<SharedString>, value: impl IntoElement, cx: &App) -> AnyElement {
    div()
        .data_font()
        .flex()
        .gap_3()
        .py_1()
        .px_2()
        .rounded_sm()
        .child(
            div()
                .w(px(180.))
                .flex_none()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(label.into()),
        )
        // `min_w_0()`: without it a flex child sized by its own content
        // (a long annotation value, an unbroken image reference) refuses
        // to shrink below that content's width and pushes the row - and
        // the panel - wider instead of wrapping.
        .child(div().flex_1().min_w_0().child(value))
        .into_any_element()
}

/// Rows with every other one on the stripe fill, so a long run of fields
/// reads as rows rather than a block of text.
pub fn striped(rows: impl IntoIterator<Item = AnyElement>, cx: &App) -> AnyElement {
    let stripe = style::stripe(cx);
    div()
        .flex()
        .flex_col()
        .children(rows.into_iter().enumerate().map(|(ix, row)| {
            let striped = ix % 2 == 1;
            div()
                .debug_selector(move || {
                    format!(
                        "detail-row-{ix}-{}",
                        if striped { "stripe" } else { "plain" }
                    )
                })
                .rounded_sm()
                .when(striped, |this| this.bg(stripe))
                .child(row)
        }))
        .into_any_element()
}

/// A heading above a group of rows, for views that stack sections rather
/// than tab between them: an accent bar beside the title, in a heavier weight.
pub fn section_heading(title: impl Into<SharedString>, cx: &App) -> AnyElement {
    div()
        .pt(crate::ui::space::spacing(cx).section_gap)
        .pb_1()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .w(px(3.))
                .h(px(14.))
                .rounded_sm()
                .bg(style::accent(cx)),
        )
        .child(div().font_weight(FontWeight::SEMIBOLD).child(title.into()))
        .into_any_element()
}

/// One chip per entry - labels, annotations, capacities.
pub fn chips(chips: &[String], cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .data_font()
        .flex()
        .flex_wrap()
        .gap_1()
        .children(chips.iter().map(|chip| {
            div()
                .px_2()
                .py_0p5()
                .rounded_md()
                .bg(theme.muted)
                .text_sm()
                .child(chip.clone())
        }))
        .into_any_element()
}

/// One badge per entry, colored by its tone - conditions, mostly.
pub fn badges<'a>(badges: impl IntoIterator<Item = (&'a str, BadgeTone)>, cx: &App) -> AnyElement {
    let background = cx.theme().background;
    div()
        .data_font()
        .flex()
        .flex_wrap()
        .gap_1()
        .children(badges.into_iter().map(|(text, tone)| {
            div()
                .px_2()
                .py_0p5()
                .rounded_full()
                .bg(tone_color(tone, cx))
                .text_color(background)
                .text_sm()
                .child(text.to_string())
        }))
        .into_any_element()
}

/// One line per entry.
pub fn lines(rows: &[String]) -> AnyElement {
    div()
        .data_font()
        .flex()
        .flex_col()
        .children(rows.iter().map(|row| div().text_sm().child(row.clone())))
        .into_any_element()
}

/// Key/value pairs, the key muted above its value - a ConfigMap's data, where
/// a value can run to many lines and would not fit beside its key.
///
/// Each key and each value has a copy control (`ui::copy`), ids
/// `copy {id_prefix} key {index}` and `copy {id_prefix} value {index}`.
pub fn key_values(id_prefix: &str, pairs: &[(String, String)], cx: &App) -> AnyElement {
    use crate::ui::copy::copyable;
    let theme = cx.theme();
    div()
        .data_font()
        .flex()
        .flex_col()
        .gap_2()
        .children(pairs.iter().enumerate().map(|(index, (key, value))| {
            let key_id = format!("copy {id_prefix} key {index}");
            let value_id = format!("copy {id_prefix} value {index}");
            div()
                .flex()
                .flex_col()
                .child(copyable(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(key.clone()),
                    ElementId::Name(key_id.clone().into()),
                    key.clone(),
                    key_id,
                ))
                .child(copyable(
                    div().code_font(cx).text_sm().child(value.clone()),
                    ElementId::Name(value_id.clone().into()),
                    value.clone(),
                    value_id,
                ))
        }))
        .into_any_element()
}

/// The events naming an object, one card each - or why there are none.
pub fn events(events: &Result<Vec<EventSummary>, String>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let events = match events {
        Ok(events) => events,
        Err(reason) => {
            return div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(format!("Could not list events: {reason}"))
                .into_any_element();
        }
    };
    if events.is_empty() {
        return div()
            .text_sm()
            .text_color(theme.muted_foreground)
            .child("No events.")
            .into_any_element();
    }
    div()
        .data_font()
        .flex()
        .flex_col()
        .gap(crate::ui::space::spacing(cx).control_gap)
        .children(events.iter().map(|event| {
            // A normal event's reason reads as plain text; only the others
            // take a status colour.
            let reason_color = match event.tone {
                BadgeTone::Good => theme.foreground,
                tone => tone_color(tone, cx),
            };
            div()
                .flex()
                .flex_col()
                .gap_1()
                .p(crate::ui::space::spacing(cx).card_padding)
                .rounded_md()
                .bg(style::surface_card(cx))
                .border_1()
                .border_color(theme.border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(reason_color)
                                .child(event.reason.clone()),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child(format!("{} · x{}", event.age, event.count)),
                        ),
                )
                .child(div().text_sm().child(event.message.clone()))
        }))
        .into_any_element()
}

/// One Secret key: its name, its size, a Show/Hide button (a tab stop, so
/// Enter or Space works on it, and danger-tinted while it would reveal),
/// and - while revealed - its value.
///
/// `reveal` is the key's current reveal, if any; `on_toggle` runs on the
/// button. `value_id` identifies the revealed value's element, for tests. The
/// value is read out of its `SecretValue` in [`revealed_text`] and nowhere
/// else in the view.
pub fn secret_key_row(
    key: &str,
    size: usize,
    reveal: Option<&Reveal>,
    button_id: ElementId,
    value_id: ElementId,
    on_toggle: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let shown = reveal.is_some();
    let button = Button::new(button_id)
        .icon(if shown {
            IconName::EyeOff
        } else {
            IconName::Eye
        })
        .label(if shown { "Hide" } else { "Show" })
        .xsmall()
        .frame_font(cx);
    // Show is tinted danger: it is about to put a sensitive value on screen.
    // Hide puts it away again, so it needs no warning.
    let button = if shown {
        button.ghost()
    } else {
        button.danger().outline()
    }
    .on_click(move |_event, window, cx| on_toggle(window, cx));
    // A revealed value can be copied - and only a revealed one: nothing on the
    // clipboard that isn't on screen.
    let copy_id = format!("copy {value_id}");
    let value = reveal.map(|reveal| {
        let text = div()
            .id(value_id)
            .code_font(cx)
            .text_sm()
            .child(revealed_text(reveal))
            .test_support();
        match reveal {
            Reveal::Shown(secret) => match secret.expose() {
                Some(plain) => crate::ui::copy::copyable(
                    text,
                    ElementId::Name(copy_id.clone().into()),
                    plain.to_string(),
                    copy_id,
                ),
                None => text.into_any_element(),
            },
            Reveal::Pending | Reveal::Failed(_) => text.into_any_element(),
        }
    });
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_sm()
                .child(div().code_font(cx).child(key.to_string()))
                .child(
                    div()
                        .text_color(theme.muted_foreground)
                        .child(format!("{size} bytes")),
                )
                .child(button),
        )
        .children(value)
        .into_any_element()
}

/// What a Secret key's value line says for `reveal`. A revealed value is shown
/// in full - Secret values never collapse.
fn revealed_text(reveal: &Reveal) -> String {
    match reveal {
        Reveal::Pending => "Revealing…".to_string(),
        Reveal::Shown(value) if value.is_empty() => "(empty)".to_string(),
        // The one place a value is read out of a `SecretValue`.
        Reveal::Shown(value) => match value.expose() {
            Some(text) => text.to_string(),
            None => format!("binary, {} bytes", value.len()),
        },
        Reveal::Failed(RevealError::Missing) => "It no longer exists.".to_string(),
        Reveal::Failed(RevealError::Failed(message)) => format!("Could not read it: {message}"),
    }
}

#[cfg(test)]
mod tests;
