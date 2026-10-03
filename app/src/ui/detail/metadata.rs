//! Label and annotation chips (`collapse-large-metadata-values`): one chip per
//! `key=value`, like [`super::chips`], except that a large value - past
//! [`COLLAPSE_VALUE_OVER_CHARS`](crate::consts::COLLAPSE_VALUE_OVER_CHARS)
//! characters or more than one line - shows only its preview, with the full
//! value in a hover tooltip.
//!
//! The rule is the Configuration tab's ([`super::collapsible::preview`]), so a
//! large value reads the same everywhere. A chip never expands in place, and
//! is not a tab stop: the panel's YAML view is the keyboard route to a value
//! in full. The tooltip is built from the value the chip was given, so a value
//! a caller redacted stays redacted in both.

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

/// The debug selector of chip `index` among `id_prefix`'s chips.
pub fn metadata_chip_selector(id_prefix: &str, index: usize) -> String {
    format!("metadata-chip {id_prefix} {index}")
}

/// The debug selector of the tooltip content of chip `index`.
pub fn metadata_tooltip_selector(id_prefix: &str, index: usize) -> String {
    format!("metadata-tooltip {id_prefix} {index}")
}

/// One chip per `(key, value)`, a large value shortened - labels and
/// annotations. `id_prefix` keeps the chips' ids apart from another field's.
pub fn metadata_chips(id_prefix: &str, pairs: &[(String, String)], cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .data_font()
        .flex()
        .flex_wrap()
        .gap_1()
        .children(pairs.iter().enumerate().map(|(index, (key, value))| {
            let chip = MetadataChip::new(key, value);
            let selector = metadata_chip_selector(id_prefix, index);
            let base = div()
                .id(metadata_chip_id(id_prefix, index))
                .debug_selector(move || selector.clone())
                .px_2()
                .py_0p5()
                .rounded_md()
                .bg(theme.muted)
                .text_sm()
                .role(accesskit::Role::Label)
                .aria_value(chip.text.clone());
            match chip.tooltip {
                // A shortened chip stays one line, so it never sets the
                // row's height.
                Some(full) => {
                    let tooltip_selector = metadata_tooltip_selector(id_prefix, index);
                    base.whitespace_nowrap()
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
                        .child(chip.text)
                        .test_support()
                        .into_any_element()
                }
                None => base.child(chip.text).test_support().into_any_element(),
            }
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests;
