//! Large values drawn collapsed (`pod-configuration-tab` 5.1): a value longer
//! than [`COLLAPSED_VALUE_PREVIEW_CHARS`] characters, or spanning more than one
//! line, shows a short preview and an Expand/Collapse button until expanded.
//!
//! Only ConfigMap values collapse: a Secret value is hidden until revealed,
//! and shown in full once it is. This module only draws; whether a value is
//! expanded is the panel's state, kept per value and never saved.

use crate::consts::COLLAPSED_VALUE_PREVIEW_CHARS;
use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::*;
use std::rc::Rc;

/// The collapsed preview of `value` - its first line's first
/// [`COLLAPSED_VALUE_PREVIEW_CHARS`] characters and an ellipsis - or `None`
/// when the value is short enough to show whole.
pub(super) fn preview(value: &str) -> Option<String> {
    let mut lines = value.lines();
    let first = lines.next().unwrap_or_default();
    let large = lines.next().is_some() || value.chars().count() > COLLAPSED_VALUE_PREVIEW_CHARS;
    large.then(|| {
        let start: String = first.chars().take(COLLAPSED_VALUE_PREVIEW_CHARS).collect();
        format!("{start}…")
    })
}

/// What the expand control runs: the panel flips the value's expanded state.
pub type OnToggle = Rc<dyn Fn(&mut Window, &mut App)>;

/// One value's element and its expand control, and whether it's expanded.
pub struct Collapsible {
    /// The drawn value's element - what a test reads the shown text from.
    pub value_id: ElementId,
    /// The Expand/Collapse button, a tab stop.
    pub toggle_id: ElementId,
    pub expanded: bool,
    pub on_toggle: OnToggle,
}

impl Collapsible {
    /// Draws `text` in full, or - while collapsed and large - as its preview,
    /// with an Expand/Collapse button. A short value gets no button.
    pub fn render(self, text: String, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let preview = preview(&text);
        let has_toggle = preview.is_some();
        let shown = match preview {
            Some(preview) if !self.expanded => preview,
            _ => text,
        };
        let value = div()
            .id(self.value_id)
            .flex_1()
            .min_w_0()
            .font_family(theme.mono_font_family.clone())
            .text_sm()
            .role(accesskit::Role::Label)
            .aria_value(shown.clone())
            .child(shown)
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
        div()
            .flex()
            .items_start()
            .gap_2()
            .child(value)
            .children(toggle)
            .into_any_element()
    }
}
