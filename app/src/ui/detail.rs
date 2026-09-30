//! The pieces every detail view draws its structured fields with: a labelled
//! row, and the value shapes a row can hold - chips, colored badges, plain
//! lines, key/value pairs, section headings and event cards.
//!
//! Shared by pod detail and the generic object viewer (`resource-links`
//! 5.1), so the two read as one application. Each panel keeps its own field
//! model; this module only draws.

use crate::k8s::resource::events::EventSummary;
use crate::k8s::resource::secret_value::{Reveal, RevealError};
use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::*;

/// A status's color, decided by the status rather than looked up at render
/// time - so "is this good" is testable without a theme, and the renderer only
/// maps tone to a color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeTone {
    /// The condition holds, or the thing is healthy.
    Good,
    /// Something to notice - not necessarily a failure.
    Warning,
    /// The cluster did not say either way.
    Unknown,
}

fn tone_color(tone: BadgeTone, cx: &App) -> Hsla {
    let theme = cx.theme();
    match tone {
        BadgeTone::Good => theme.success,
        BadgeTone::Warning => theme.warning,
        BadgeTone::Unknown => theme.muted_foreground,
    }
}

/// One field row: a fixed-width muted label, and the value beside it.
pub fn row(label: impl Into<SharedString>, value: impl IntoElement, cx: &App) -> AnyElement {
    div()
        .flex()
        .gap_3()
        .py_1()
        .child(
            div()
                .w(px(180.))
                .flex_none()
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

/// A heading above a group of rows, for views that stack sections rather
/// than tab between them.
pub fn section_heading(title: impl Into<SharedString>, cx: &App) -> AnyElement {
    div()
        .pt_3()
        .pb_1()
        .border_b_1()
        .border_color(cx.theme().border)
        .font_weight(FontWeight::MEDIUM)
        .child(title.into())
        .into_any_element()
}

/// One chip per entry - labels, annotations, capacities.
pub fn chips(chips: &[String], cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
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
        .flex()
        .flex_col()
        .children(rows.iter().map(|row| div().text_sm().child(row.clone())))
        .into_any_element()
}

/// Key/value pairs, the key muted above its value - a ConfigMap's data, where
/// a value can run to many lines and would not fit beside its key.
pub fn key_values(pairs: &[(String, String)], cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .flex()
        .flex_col()
        .gap_2()
        .children(pairs.iter().map(|(key, value)| {
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(key.clone()),
                )
                .child(
                    div()
                        .font_family(theme.mono_font_family.clone())
                        .text_sm()
                        .child(value.clone()),
                )
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
        .flex()
        .flex_col()
        .gap_2()
        .children(events.iter().map(|event| {
            let reason_color = match event.tone {
                BadgeTone::Good => theme.foreground,
                BadgeTone::Warning => theme.warning,
                BadgeTone::Unknown => theme.muted_foreground,
            };
            div()
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .rounded_md()
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
/// Enter or Space works on it), and - while revealed - its value.
///
/// `reveal` is the key's current reveal, if any; `on_toggle` runs on the
/// button. `value_id` identifies the revealed value's element, for tests. The
/// value is read out of its `SecretValue` here and nowhere else in the view.
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
        .ghost()
        .on_click(move |_event, window, cx| on_toggle(window, cx));
    let value = reveal.map(|reveal| {
        let text = match reveal {
            Reveal::Pending => "Revealing…".to_string(),
            Reveal::Shown(value) if value.is_empty() => "(empty)".to_string(),
            // The one place a value is read out of a `SecretValue`.
            Reveal::Shown(value) => match value.expose() {
                Some(text) => text.to_string(),
                None => format!("binary, {} bytes", value.len()),
            },
            Reveal::Failed(RevealError::Missing) => "It no longer exists.".to_string(),
            Reveal::Failed(RevealError::Failed(message)) => format!("Could not read it: {message}"),
        };
        div()
            .id(value_id)
            .font_family(theme.mono_font_family.clone())
            .text_sm()
            .child(text)
            .test_support()
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
                .child(
                    div()
                        .font_family(theme.mono_font_family.clone())
                        .child(key.to_string()),
                )
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
