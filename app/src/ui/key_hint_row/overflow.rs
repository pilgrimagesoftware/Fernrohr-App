//! A collapsed row's overflow control (`key-hint-rows` D3, D4): an icon
//! button, last in the row, whose tooltip lists the hidden hints with their
//! live keys and names the Key Hints key. It is a tab stop; Enter or Space,
//! like a click, opens the Key Hints overlay, which lists every key available
//! where focus is - the hidden ones included.

use crate::util::key_hints::{SHOW_KEY_HINTS_KEY, ShowKeyHints};
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;

/// The key context around the overflow control's button, which [`init`]'s
/// interceptor looks for.
const KEY_CONTEXT: &str = "KeyHintOverflow";

/// Lets Enter and Space activate a focused overflow control. A button acts on
/// a key's release, but key bindings are matched on its press, first: in a
/// panel that binds Space (Pods' Quick Look) or Enter (a list's Open), the
/// panel's command would run instead. While focus is on the control, those
/// two keys are its own, as they are for any focused button; nothing is bound,
/// so no other key or context changes.
pub fn init(cx: &mut App) {
    cx.intercept_keystrokes(|event, window, cx| {
        let activates = matches!(event.keystroke.key.as_str(), "enter" | "space")
            && !event.keystroke.modifiers.modified();
        let on_control = event
            .context_stack
            .iter()
            .any(|context| context.contains(KEY_CONTEXT));
        if activates && on_control {
            window.dispatch_action(Box::new(ShowKeyHints), cx);
            cx.stop_propagation();
        }
    })
    .detach();
}

/// The overflow control's tooltip title.
pub(crate) const TOOLTIP: &str = "More keys";

/// The id of row `row`'s overflow button.
fn button_id(row: &str) -> SharedString {
    format!("{row}-more").into()
}

/// The debug selector of row `row`'s overflow control.
pub(crate) fn control_selector(row: &str) -> String {
    format!("key-hint-row {row} more")
}

/// The overflow control for row `row`, listing `hidden` hints.
pub(super) fn control(
    row: &SharedString,
    hidden: Vec<(Kbd, SharedString)>,
    window: &Window,
) -> Stateful<Div> {
    let every_key = super::live_key(&ShowKeyHints, None, SHOW_KEY_HINTS_KEY, window);
    let button = Button::new(ElementId::Name(button_id(row)))
        .icon(IconName::Ellipsis)
        .xsmall()
        .ghost()
        .on_click(|_, window, cx| window.dispatch_action(Box::new(ShowKeyHints), cx));
    div()
        .id(ElementId::Name(format!("{row}-more-tooltip").into()))
        .flex_shrink_0()
        .key_context(KEY_CONTEXT)
        .debug_selector({
            let selector = control_selector(row);
            move || selector
        })
        .tooltip(move |window, cx| {
            let hidden = hidden.clone();
            let every_key = every_key.clone();
            Tooltip::element(move |_, _| tooltip(&hidden, every_key.clone())).build(window, cx)
        })
        .child(button)
}

/// "More keys", each hidden hint's key and label in row order, then where to
/// find every key.
fn tooltip(hidden: &[(Kbd, SharedString)], every_key: Kbd) -> Div {
    div()
        .debug_selector(|| crate::ui::icon_tooltip::selector(TOOLTIP))
        .flex()
        .flex_col()
        .gap_1()
        .child(TOOLTIP)
        .children(hidden.iter().map(|(key, label)| {
            super::KeyHint::body(key.clone(), label.clone()).debug_selector({
                let label = label.clone();
                move || hidden_selector(&label)
            })
        }))
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child("Press")
                .child(every_key)
                .child("for every key"),
        )
}

/// The debug selector of the tooltip's line for the hidden hint `label`.
pub(crate) fn hidden_selector(label: &str) -> String {
    format!("key-hint-row hidden {label}")
}
