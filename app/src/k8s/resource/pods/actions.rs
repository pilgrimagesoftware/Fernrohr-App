//! Acting on the selected pod (`k9s-remaining-keybindings` section 1): Delete,
//! which asks first, and Kill, which doesn't - a zero-grace delete for the
//! restart-it-now workflow. Both go through `resource_actions::delete`.
//!
//! A refused delete shows as a banner above the table, naming the pod and the
//! server's reason; the row stays, since nothing was deleted. A delete that
//! succeeds needs nothing here: the watch drops the row when the pod goes.

use super::*;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::resource_actions::{self, ActionFailure};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;

/// The Delete dialog's buttons.
pub(super) const DELETE_CANCEL_ID: &str = "pods-delete-cancel";
pub(super) const DELETE_CONFIRM_ID: &str = "pods-delete-confirm";
/// The failure banner, and its Dismiss button.
pub(super) const FAILURE_ID: &str = "pods-action-failure";
pub(super) const DISMISS_FAILURE_ID: &str = "pods-action-failure-dismiss";

/// An action the cluster refused: what was asked, and why it was refused.
#[derive(Clone, Debug)]
pub(super) struct PodActionFailure {
    pub(super) action: String,
    pub(super) failure: ActionFailure,
}

impl PodsPanel {
    /// `DeletePod`: asks before deleting the selected pod.
    pub(super) fn on_action_delete_pod(
        &mut self,
        _: &DeletePod,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.table_selection(cx) else {
            return;
        };
        let panel = cx.weak_entity();
        let body = format!(
            "Delete pod {} in {}? A controller that owns it may start a replacement.",
            target.name, target.namespace
        );
        window.open_dialog(cx, move |dialog, _window, _cx| {
            let (panel, target) = (panel.clone(), target.clone());
            dialog.title("Delete Pod?").child(body.clone()).footer(
                DialogFooter::new()
                    .child(Button::new(DELETE_CANCEL_ID).label("Cancel").on_click(
                        |_event, window, cx| {
                            window.close_dialog(cx);
                        },
                    ))
                    .child(
                        Button::new(DELETE_CONFIRM_ID)
                            .label("Delete")
                            .with_variant(ButtonVariant::Danger)
                            .on_click(move |_event, window, cx| {
                                window.close_dialog(cx);
                                let _ = panel.update(cx, |panel, cx| {
                                    panel.delete_pod(target.clone(), false, cx);
                                });
                            }),
                    ),
            )
        });
    }

    /// `KillPod`: deletes the selected pod at once, with no grace period and
    /// no prompt.
    pub(super) fn on_action_kill_pod(
        &mut self,
        _: &KillPod,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(target) = self.table_selection(cx) {
            self.delete_pod(target, true, cx);
        }
    }

    /// Sends the delete - `force` for a kill - and shows a refusal when it lands.
    fn delete_pod(&mut self, target: PodSelection, force: bool, cx: &mut Context<Self>) {
        let crate::k8s::cluster::connection::ConnectionState::Connected(client) =
            &self.connection.read(cx).state
        else {
            return;
        };
        let client = client.clone();
        self.action_failure = None;
        let action = format!(
            "{} pod {}",
            if force { "Kill" } else { "Delete" },
            target.name
        );
        let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
            let result = resource_actions::delete(
                client,
                &DiscoveredKind::pods(),
                &target.name,
                Some(&target.namespace),
                force,
            )
            .await;
            let _ = tx.send(result).await;
        });
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, |result| {
                let _ = this.update(cx, |this, cx| {
                    this.action_failure = result.err().map(|failure| PodActionFailure {
                        action: action.clone(),
                        failure,
                    });
                    cx.notify();
                });
            })
            .await;
        })
        .detach();
        cx.notify();
    }

    /// The banner for a refused action, above the table.
    pub(super) fn render_action_failure(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let PodActionFailure { action, failure } = self.action_failure.clone()?;
        let space = crate::ui::space::spacing(cx);
        let this = cx.weak_entity();
        Some(
            div()
                .id(FAILURE_ID)
                .debug_selector(|| FAILURE_ID.into())
                .flex()
                .items_start()
                .gap(space.control_gap)
                .mb(space.control_gap)
                .px(space.panel_inset)
                .py(space.control_gap)
                .rounded_md()
                .border_1()
                .border_color(crate::ui::style::status(crate::ui::style::Tone::Bad, cx))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_color(crate::ui::style::status(
                                    crate::ui::style::Tone::Bad,
                                    cx,
                                ))
                                .child(format!("{action} failed: {}", failure.message)),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(failure.detail),
                        ),
                )
                .child(
                    Button::new(DISMISS_FAILURE_ID)
                        .label("Dismiss")
                        .ghost()
                        .on_click(move |_event, _window, cx| {
                            let _ = this.update(cx, |this, cx| {
                                this.action_failure = None;
                                cx.notify();
                            });
                        }),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests;
