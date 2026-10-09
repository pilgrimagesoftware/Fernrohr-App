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
    with_tooltip_text(id, text, button)
}

/// [`with_tooltip`] for text built at render time, such as a state with how
/// long it has lasted.
pub fn with_tooltip_text(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    element: impl IntoElement,
) -> Stateful<Div> {
    let text: SharedString = text.into();
    div()
        .id(id.into())
        .tooltip(move |window, cx| {
            let text = text.clone();
            Tooltip::element(move |_, _| {
                let label = text.clone();
                div()
                    .debug_selector(move || selector(&label))
                    .child(text.clone())
            })
            .build(window, cx)
        })
        .child(element)
}
