//! Large values drawn collapsed (`pod-configuration-tab` 5.1): a value longer
//! than [`COLLAPSE_VALUE_OVER_CHARS`] characters, or spanning more than one
//! line, shows a short preview - its first line's first
//! [`COLLAPSED_VALUE_PREVIEW_CHARS`] characters - and an Expand/Collapse button
//! until expanded.
//!
//! Only ConfigMap values collapse: a Secret value is hidden until revealed,
//! and shown in full once it is. This module only draws; whether a value is
//! expanded is the panel's state, kept per value and never saved.

use crate::consts::{COLLAPSE_VALUE_OVER_CHARS, COLLAPSED_VALUE_PREVIEW_CHARS};
use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::*;
use std::rc::Rc;

/// The collapsed preview of `value` - its first line's first
/// [`COLLAPSED_VALUE_PREVIEW_CHARS`] characters and an ellipsis - or `None`
/// when the value is short enough to show whole: one line of at most
/// [`COLLAPSE_VALUE_OVER_CHARS`] characters.
pub(super) fn preview(value: &str) -> Option<String> {
    let mut lines = value.lines();
    let first = lines.next().unwrap_or_default();
    let large = lines.next().is_some() || value.chars().count() > COLLAPSE_VALUE_OVER_CHARS;
    large.then(|| {
        let start: String = first.chars().take(COLLAPSED_VALUE_PREVIEW_CHARS).collect();
        format!("{start}…")
    })
}

/// What the expand control runs: the panel flips the value's expanded state.
pub type OnToggle = Rc<dyn Fn(&mut Window, &mut App)>;

/// One key/value pair whose value may collapse: the element ids, and whether
/// it's expanded.
pub struct Collapsible {
    /// The key's label - what a test measures the control's place against.
    pub key_id: ElementId,
    /// The drawn value's element - what a test reads the shown text from.
    pub value_id: ElementId,
    /// The Expand/Collapse button, a tab stop.
    pub toggle_id: ElementId,
    pub expanded: bool,
    pub on_toggle: OnToggle,
}

impl Collapsible {
    /// Draws `key` above `text`, the value in full or - while collapsed and
    /// large - as its preview. A large value's Expand/Collapse button sits
    /// right after the key, in the same place either way, so a wide panel
    /// doesn't push it out of sight; a short value gets no button.
    pub fn render(self, key: &str, text: String, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let preview = preview(&text);
        let has_toggle = preview.is_some();
        let shown = match preview {
            Some(preview) if !self.expanded => preview,
            _ => text,
        };
        let label = div()
            .id(self.key_id)
            .text_sm()
            .text_color(theme.muted_foreground)
            .child(key.to_string())
            .test_support();
        let toggle = has_toggle.then(|| {
            let on_toggle = self.on_toggle;
            Button::new(self.toggle_id)
                .icon(if self.expanded {
                    IconName::ChevronUp
                } else {
                    IconName::ChevronDown
                })
                .label(if self.expanded { "Collapse" } else { "Expand" })
                .xsmall()
                .ghost()
                .on_click(move |_event, window, cx| on_toggle(window, cx))
        });
        let value = div()
            .id(self.value_id)
            .font_family(theme.mono_font_family.clone())
            .text_sm()
            .role(accesskit::Role::Label)
            .aria_value(shown.clone())
            .child(shown)
            .test_support();
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(label)
                    .children(toggle),
            )
            .child(value)
            .into_any_element()
    }
}
