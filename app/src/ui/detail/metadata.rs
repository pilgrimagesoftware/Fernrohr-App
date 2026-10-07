//! Label and annotation chips (`collapse-large-metadata-values`): one chip per
//! `key=value`, like [`super::chips`], except that a large value - past
//! [`COLLAPSE_VALUE_OVER_CHARS`](crate::consts::COLLAPSE_VALUE_OVER_CHARS)
//! characters or more than one line - shows only its preview, with the full
//! value in a hover tooltip.
//!
//! The rule is the Configuration tab's ([`super::collapsible::preview`]), so a
//! large value reads the same everywhere. A chip never expands in place. The
//! keyboard's route to a shortened value is the copy control beside it
//! (`ui::copy`), a tab stop that puts the full value on the clipboard - and
//! the panel's YAML view, which shows every value in full. The tooltip and the
//! copy control are built from the value the chip was given, so a value a
//! caller redacted stays redacted in all three.
//!
//! A chip copies itself (#153): clicking it, or Enter or Space while the chip
//! has keyboard focus - each chip is a tab stop - puts its whole `key=value`
//! on the clipboard, a shortened value in full.

use super::collapsible::preview;
use crate::ui::typography::TypeRole as _;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;

/// What a metadata chip shows: `key=value`, or `key=` and the value's preview,
/// and - only when shortened - the full value its tooltip carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataChip {
    pub text: String,
    pub tooltip: Option<String>,
}

impl MetadataChip {
    pub fn new(key: &str, value: &str) -> Self {
        match preview(value) {
            Some(preview) => Self {
                text: format!("{key}={preview}"),
                tooltip: Some(value.to_string()),
            },
            None => Self {
                text: format!("{key}={value}"),
                tooltip: None,
            },
        }
    }
}

/// The element of chip `index` among `id_prefix`'s chips - what a test reads
/// the shown text from.
pub fn metadata_chip_id(id_prefix: &str, index: usize) -> ElementId {
    ElementId::Name(format!("metadata-chip {id_prefix} {index}").into())
}

/// The copy control beside shortened chip `index` among `id_prefix`'s chips.
pub fn metadata_copy_id(id_prefix: &str, index: usize) -> ElementId {
    ElementId::Name(format!("copy metadata {id_prefix} {index}").into())
}

/// The debug selector of chip `index` among `id_prefix`'s chips.
pub fn metadata_chip_selector(id_prefix: &str, index: usize) -> String {
    format!("metadata-chip {id_prefix} {index}")
}

/// The debug selector of the tooltip content of chip `index`.
pub fn metadata_tooltip_selector(id_prefix: &str, index: usize) -> String {
    format!("metadata-tooltip {id_prefix} {index}")
}

/// What an unshortened chip's tooltip says it does.
pub const COPY_CHIP_TOOLTIP: &str = "Click to copy";

/// One chip per `(key, value)`, a large value shortened - labels and
/// annotations. `id_prefix` keeps the chips' ids apart from another field's.
pub fn metadata_chips(id_prefix: &str, pairs: &[(String, String)]) -> AnyElement {
    div()
        .data_font()
        .flex()
        .flex_wrap()
        .gap_1()
        .children(
            pairs
                .iter()
                .enumerate()
                .map(|(index, (key, value))| ChipElement {
                    id_prefix: id_prefix.to_string(),
                    index,
                    key: key.clone(),
                    value: value.clone(),
                }),
        )
        .into_any_element()
}

/// One chip, an element of its own so it can keep the focus handle that makes
/// it a tab stop - per chip id, across renders, as a `Button` keeps its own.
#[derive(IntoElement)]
struct ChipElement {
    id_prefix: String,
    index: usize,
    key: String,
    value: String,
}

impl RenderOnce for ChipElement {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            id_prefix,
            index,
            key,
            value,
        } = self;
        let id = metadata_chip_id(&id_prefix, index);
        // The handle itself is the tab stop: an element tracking an explicit
        // handle doesn't pass its own `tab_index` on to it.
        let focus = window
            .use_keyed_state(id.clone(), cx, |_, cx| cx.focus_handle().tab_stop(true))
            .read(cx)
            .clone();
        let accent = crate::ui::style::accent(cx);
        let chip = MetadataChip::new(&key, &value);
        let selector = metadata_chip_selector(&id_prefix, index);
        let pair = format!("{key}={value}");
        let base = div()
            .id(id)
            .debug_selector(move || selector.clone())
            .px_2()
            .py_0p5()
            .rounded_md()
            .bg(cx.theme().muted)
            .text_sm()
            // Copies itself on a click, or Enter or Space once Tab has
            // reached it - a ring shows which chip that is.
            .test_support()
            .track_focus(&focus)
            .tab_index(0)
            .cursor_pointer()
            .border_1()
            .border_color(transparent_black())
            .focus_visible(|style| style.border_color(accent))
            .on_click(move |_, _, cx| crate::ui::copy::copy_text(&pair, cx))
            .role(accesskit::Role::Label)
            .aria_value(chip.text.clone());
        match chip.tooltip {
            // A shortened chip stays one line, so it never sets the
            // row's height.
            Some(full) => {
                let tooltip_selector = metadata_tooltip_selector(&id_prefix, index);
                let group = format!("metadata {id_prefix} {index}");
                let copy = crate::ui::copy::copy_button(
                    metadata_copy_id(&id_prefix, index),
                    full.clone(),
                    group.clone(),
                );
                let chip = base
                    .whitespace_nowrap()
                    .tooltip(move |window, cx| {
                        let (full, selector) = (full.clone(), tooltip_selector.clone());
                        Tooltip::element(move |_, cx| {
                            // Wrapped and capped: the YAML view is where a
                            // value is read in full, not a screen-sized popup.
                            div()
                                .debug_selector({
                                    let selector = selector.clone();
                                    move || selector
                                })
                                .max_w(px(480.))
                                .max_h(px(320.))
                                .overflow_hidden()
                                .whitespace_normal()
                                .code_font(cx)
                                .child(full.clone())
                        })
                        .build(window, cx)
                    })
                    .child(chip.text);
                div()
                    .group(group)
                    .flex()
                    .items_center()
                    .child(chip)
                    .child(copy)
                    .into_any_element()
            }
            None => base
                .tooltip(|window, cx| Tooltip::new(COPY_CHIP_TOOLTIP).build(window, cx))
                .child(chip.text)
                .into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests;
