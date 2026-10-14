//! An error shown in a panel's content, with Report… beside it (#177): the
//! message and its detail as selectable text. Split out of `title.rs`, which
//! owns what a panel's title bar shows.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::*;

/// A panel's failure content: a human-readable message, then - when there is
/// one - the failure's full technical detail underneath it, muted and
/// monospace. Both render through `gpui-component`'s own selectable
/// [`gpui_kit::component::text::TextView`] (`.selectable(true)`), the same
/// primitive its own dialogs and message views use for copyable prose - not a
/// hand-rolled selection - so a user can drag-select either line and press
/// `Cmd+C`/`Ctrl+C` (the component's own binding, `text::state::CONTEXT`) to
/// copy it. `1-window-context-bar` bug 2: a raw `{error:?}` dump was neither
/// readable nor selectable; this is Logs' and Pod detail's shared fix for
/// both halves at once, so the two panels cannot drift back apart on either.
///
/// Below them, Report… (#177) opens Report Issue with the message and detail
/// already filled in - a tab stop, so Enter or Space reaches it as a click
/// does.
pub fn error_content(message: String, detail: Option<String>, cx: &App) -> impl IntoElement {
    let space = crate::ui::space::spacing(cx);
    let report = crate::ui::report_issue::ReportError::of(&message, detail.as_deref());
    div()
        .size_full()
        .p(space.panel_inset)
        .flex()
        .flex_col()
        .gap(space.control_gap)
        .child(gpui_kit::component::text::markdown(escape_markdown(&message)).selectable(true))
        .children(detail.map(|detail| {
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(gpui_kit::component::text::markdown(code_block(&detail)).selectable(true))
        }))
        .child(div().flex().child(report_button(report)))
}

/// The id of an error's Report… button, for tests.
pub const REPORT_ERROR_BUTTON: &str = "report-error";

/// Report…: opens Report Issue filled in with `report`.
pub fn report_button(report: crate::ui::report_issue::ReportError) -> Button {
    Button::new(REPORT_ERROR_BUTTON)
        .label("Report\u{2026}")
        .link()
        .small()
        .tooltip("Report this error, its text filled in")
        .on_click(move |_event, window, cx| window.dispatch_action(Box::new(report.clone()), cx))
}

/// `text` with every markdown-significant character backslash-escaped, so the only
/// selectable-text primitive gpui-component offers (markdown) shows it verbatim.
pub(crate) fn escape_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if "\\`*_{}[]()<>#+-.!|~".contains(ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// `text` as a fenced code block - rendered verbatim and in monospace. The fence is
/// longer than any backtick run inside `text`, so nothing in it can close the block.
pub(super) fn code_block(text: &str) -> String {
    let longest_run = text.split(|ch| ch != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest_run.max(2) + 1);
    format!("{fence}text\n{text}\n{fence}")
}
