//! A tooltip for an icon button (`.claude/rules/icon-buttons.md`) that tests can
//! find: the button inside a wrapper whose tooltip names the action and carries a
//! debug selector, so a test hovers the button and asserts the text, failing if
//! the tooltip went missing. gpui-kit's own button tooltip has no such hook.

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;

/// The debug selector of the tooltip reading `text`.
pub fn selector(text: &str) -> String {
    format!("tooltip {text}")
}

/// `button` with a tooltip reading `text`, on a wrapper identified by `id`.
pub fn with_tooltip(
    id: impl Into<ElementId>,
    text: &'static str,
    button: impl IntoElement,
) -> Stateful<Div> {
    div()
        .id(id.into())
        .tooltip(move |window, cx| {
            Tooltip::element(move |_, _| div().debug_selector(move || selector(text)).child(text))
                .build(window, cx)
        })
        .child(button)
}
