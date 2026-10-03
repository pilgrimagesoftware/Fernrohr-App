//! Copying from the detail panels (`resource-detail-ui-improvements` 3): the
//! Copy Resource Name command, and a copy control beside a copyable value - a
//! container image, a ConfigMap key or value, a revealed Secret value.
//!
//! The control sits beside its value, faint until the row is hovered, so a row
//! of values doesn't read as a row of buttons. It is a tab stop, so the
//! keyboard reaches it and Space or Enter copies, as a click does.

use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::*;

actions!(copy, [CopyResourceName]);

/// Copy Resource Name's default key in each detail panel.
pub const COPY_NAME_KEY: &str = "c";

/// How visible a copy control is while its row isn't hovered.
const RESTING_OPACITY: f32 = 0.35;

/// Puts `text` on the clipboard.
pub fn copy_text(text: &str, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
}

/// A copy control for `value`, `id` its element id. It brightens while the
/// element with `group` - the value's row - is hovered.
pub fn copy_button(
    id: impl Into<ElementId>,
    value: String,
    group: impl Into<SharedString>,
) -> AnyElement {
    let group = group.into();
    div()
        .flex_none()
        .opacity(RESTING_OPACITY)
        .group_hover(group, |style| style.opacity(1.))
        .child(
            Button::new(id)
                .icon(IconName::Copy)
                .xsmall()
                .ghost()
                .tooltip("Copy")
                .on_click(move |_event, _window, cx| copy_text(&value, cx)),
        )
        .into_any_element()
}

/// `content` with a copy control for `value` after it, the pair one hover
/// group named `group`.
pub fn copyable(
    content: impl IntoElement,
    id: impl Into<ElementId>,
    value: String,
    group: impl Into<SharedString>,
) -> AnyElement {
    let group = group.into();
    div()
        .group(group.clone())
        .flex()
        .items_center()
        .gap_1()
        .min_w_0()
        .w_full()
        .child(content)
        .child(copy_button(id, value, group))
        .into_any_element()
}
