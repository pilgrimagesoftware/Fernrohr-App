//! The Settings window's Tools section (`helm-releases` 7.1): which `helm`
//! Helm release actions run. The field holds the path; Enter, or leaving the
//! field, saves it and checks the binary again (`helm_cli::set_path`). Empty
//! means the `helm` on the login shell's PATH. Below it, what the last check
//! found, or why nothing usable was.

use super::card::{body, card, hint};
use crate::helm_cli::{self, HelmStatus};
use crate::ui::detail::{BadgeTone, tone_color};
use crate::ui::typography::TypeRole as _;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;

/// The helm path field's element id and debug selector.
pub(crate) const HELM_PATH_FIELD: &str = "settings-helm-path";
/// The line saying what the last check for `helm` found.
pub(crate) const HELM_STATUS: &str = "settings-helm-status";

pub struct ToolsSection {
    helm_path: Entity<InputState>,
    _helm_path_events: Subscription,
}

impl ToolsSection {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let current = helm_cli::current_path(cx).unwrap_or_default();
        let helm_path = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("helm on your login shell's PATH")
                .default_value(current)
        });
        let _helm_path_events = cx.subscribe(&helm_path, |_this, input, event, cx| {
            if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                let path = input.read(cx).value();
                helm_cli::set_path(&path, cx);
                cx.notify();
            }
        });
        Self {
            helm_path,
            _helm_path_events,
        }
    }

    /// The field, so showing the section can focus it.
    pub(super) fn field_focus(&self, cx: &App) -> FocusHandle {
        self.helm_path.read(cx).focus_handle(cx)
    }
}

/// The status line's text and tone for `status`.
fn status_line(status: &HelmStatus) -> (String, BadgeTone) {
    match status {
        HelmStatus::Checking => ("Looking for helm…".into(), BadgeTone::Unknown),
        HelmStatus::Ready(binary) => (
            format!("Found helm {} at {}", binary.version, binary.path.display()),
            BadgeTone::Good,
        ),
        HelmStatus::Unusable(reason) => (reason.clone(), BadgeTone::Warning),
    }
}

impl Render for ToolsSection {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (text, tone) = status_line(&helm_cli::status(cx));
        body(cx).child(
            card("Helm", cx)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(crate::ui::space::spacing(cx).control_gap)
                        .child(div().w(rems(7.5)).child("helm path"))
                        .child(
                            div()
                                .id(HELM_PATH_FIELD)
                                .debug_selector(|| HELM_PATH_FIELD.into())
                                .flex_1()
                                .child(crate::ui::text_field::text_field(&self.helm_path, cx)),
                        ),
                )
                .child(
                    div()
                        .debug_selector(|| HELM_STATUS.into())
                        .data_font()
                        .text_sm()
                        .text_color(tone_color(tone, cx))
                        .child(text),
                )
                .child(hint(
                    "Rolling back or uninstalling a Helm release runs this helm, which must be \
                     Helm 3 or 4. Leave it empty to use the helm on your login shell's PATH. Enter \
                     saves it.",
                    cx,
                )),
        )
    }
}
