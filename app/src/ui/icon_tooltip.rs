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

/// [`with_tooltip`] for a tooltip whose text is built at render time - a state's
/// elapsed time, say - rather than a fixed action name.
pub fn with_text_tooltip(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    child: impl IntoElement,
) -> Stateful<Div> {
    let text: SharedString = text.into();
    div()
        .id(id.into())
        .tooltip(move |window, cx| {
            let text = text.clone();
            Tooltip::element(move |_, _| {
                let selector = selector(&text);
                div().debug_selector(move || selector).child(text.clone())
            })
            .build(window, cx)
        })
        .child(child)
}
