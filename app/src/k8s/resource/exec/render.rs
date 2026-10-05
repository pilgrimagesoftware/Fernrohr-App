//! Drawing a shell: the transcript, scrolled to its end as output arrives;
//! below it the input line while the session runs, or a note that it ended -
//! the transcript stays readable either way.

use super::panel::{ExecPanel, SessionState};
use crate::ui::typography::TypeRole as _;
use gpui_kit::base::FocusTrapElement as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::input::Input;
use gpui_kit::*;

/// Debug selectors: the transcript, and the ended note.
pub(crate) const TRANSCRIPT: &str = "exec-transcript";
pub(crate) const ENDED: &str = "exec-ended";

impl Render for ExecPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let space = crate::ui::space::spacing(cx);
        let footer = match &self.state {
            SessionState::Ended(reason) => div()
                .debug_selector(|| ENDED.into())
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(match reason {
                    Some(reason) => format!("Session ended: {reason}"),
                    None => "Session ended.".to_string(),
                })
                .into_any_element(),
            SessionState::Waiting => div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Waiting for the cluster connection…")
                .into_any_element(),
            SessionState::Running => Input::new(&self.input).into_any_element(),
        };
        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .p(space.panel_inset)
            .flex()
            .flex_col()
            .gap(space.control_gap)
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "{} · {} in {}",
                        self.target.pod, self.target.container, self.target.namespace
                    )),
            )
            .child(
                div()
                    .id("exec-transcript-scroll")
                    .debug_selector(|| TRANSCRIPT.into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .p(space.control_gap)
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .code_font(cx)
                    .whitespace_normal()
                    .child(self.transcript.clone()),
            )
            .child(footer)
            // Tab stays in the panel: see `ui::panel::focus`.
            .focus_trap("exec-panel-tab-trap", &self.focus_handle)
    }
}
