//! Drawing a shell: the terminal, filling the panel, and below it the keys the
//! app keeps while the terminal has focus - or, once the session is over, how
//! it ended, the screen and scrollback still there to read and copy.
//!
//! Key ownership (`embedded-exec-terminal` decision 2): the panel's key
//! context is [`KEY_CONTEXT`], and it also carries `Input`, as a text field
//! does - so every app key bound `… && !Input` (every plain and `ctrl-` key)
//! stays out, and the terminal gets them. What the app keeps are its `cmd-`
//! keys: the palette, panel and tab navigation, and copy and paste, which the
//! terminal handles itself.

use super::panel::{ExecPanel, SessionState};
use gpui_kit::base::FocusTrapElement as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;

/// The shell panel's key context.
pub const KEY_CONTEXT: &str = "ExecTerminal";
/// Debug selectors: the terminal, the reserved-key hints, the ended notice.
pub(crate) const TERMINAL: &str = "exec-terminal";
pub(crate) const HINTS: &str = "exec-hints";
pub(crate) const ENDED: &str = "exec-ended";
pub(crate) const INPUT_NOTICE: &str = "exec-input-notice";

impl ExecPanel {
    /// Keeps the terminal in the theme's font, size and colours - a no-op
    /// unless one changed, when the terminal re-measures and resizes.
    fn sync_style(&self, cx: &mut Context<Self>) {
        if let Some(terminal) = &self.terminal {
            let style = super::theme::terminal_style(cx);
            terminal.update(cx, |view, cx| view.set_style(style, cx));
        }
    }

    /// Focus given to the panel goes to its terminal, where keys are read.
    fn pass_focus_to_terminal(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(terminal) = &self.terminal else {
            return;
        };
        if self.focus_handle.is_focused(window) {
            let focus = terminal.focus_handle(cx);
            window.defer(cx, move |window, cx| window.focus(&focus, cx));
        }
    }

    /// The keys the app keeps while the terminal has focus.
    fn hints(&self, window: &Window, cx: &App) -> AnyElement {
        let key = |keys: &str| Kbd::new(Keystroke::parse(keys).expect("valid keystroke"));
        let (copy, paste) = if cfg!(target_os = "macos") {
            ("cmd-c", "cmd-v")
        } else {
            ("ctrl-shift-c", "ctrl-shift-v")
        };
        let palette =
            Kbd::binding_for_action(&crate::util::shell::ToggleCommandPalette, None, window)
                .unwrap_or_else(|| key(crate::util::shell::TOGGLE_PALETTE_DEFAULT_BINDING));
        let hint = |kbd: Kbd, label: &'static str| {
            div().flex().items_center().gap_1().child(kbd).child(label)
        };
        div()
            .debug_selector(|| HINTS.into())
            .flex()
            .flex_wrap()
            .gap(crate::ui::space::spacing(cx).control_gap)
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(hint(key(copy), "Copy"))
            .child(hint(key(paste), "Paste"))
            .child(hint(palette, "Commands"))
            .into_any_element()
    }
}

impl Render for ExecPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_style(cx);
        self.pass_focus_to_terminal(window, cx);
        let space = crate::ui::space::spacing(cx);
        let muted = cx.theme().muted_foreground;
        let footer = match &self.state {
            SessionState::Ended(end) => div()
                .debug_selector(|| ENDED.into())
                .text_sm()
                .text_color(muted)
                .child(ended_notice(end.code, end.reason.as_deref()))
                .into_any_element(),
            SessionState::Waiting => div()
                .text_sm()
                .text_color(muted)
                .child("Waiting for the cluster connection…")
                .into_any_element(),
            SessionState::Running => self.hints(window, cx),
        };
        let mut context = KeyContext::default();
        context.add(KEY_CONTEXT);
        context.add("Input");
        div()
            .size_full()
            .key_context(context)
            .track_focus(&self.focus_handle)
            .p(space.panel_inset)
            .flex()
            .flex_col()
            .gap(space.control_gap)
            .child(div().text_sm().text_color(muted).child(format!(
                "{} · {} in {}",
                self.target.pod, self.target.container, self.target.namespace
            )))
            .child(
                div()
                    .debug_selector(|| TERMINAL.into())
                    .flex_1()
                    .min_h_0()
                    .rounded_md()
                    .overflow_hidden()
                    .border_1()
                    .border_color(cx.theme().border)
                    .children(self.terminal.clone()),
            )
            .children(self.input_notice.clone().map(|notice| {
                div()
                    .debug_selector(|| INPUT_NOTICE.into())
                    .text_sm()
                    .text_color(crate::ui::style::status(
                        crate::ui::style::Tone::Warning,
                        cx,
                    ))
                    .child(format!("Input didn't reach the shell: {notice}"))
            }))
            .child(footer)
            // Tab goes to the shell, not around the panel: the terminal is a
            // tab stop of its own and reads Tab as a key.
            .focus_trap("exec-panel-tab-trap", &self.focus_handle)
    }
}

/// The notice under an ended session: how it ended - its exit status, or why.
pub(crate) fn ended_notice(code: Option<i32>, reason: Option<&str>) -> String {
    match (code, reason) {
        (_, Some(reason)) => format!("Session ended: {reason}"),
        (Some(code), None) => format!("Session ended (exit status {code})."),
        (None, None) => "Session ended.".to_string(),
    }
}
