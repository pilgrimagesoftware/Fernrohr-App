//! Acting on the selected pod (`k9s-remaining-keybindings` section 1): Delete,
//! which asks first, and Kill, which doesn't - a zero-grace delete for the
//! restart-it-now workflow. Both go through `delete_flow`, as every panel's
//! delete does.
//!
//! A refused delete shows as a banner above the table, naming the pod and the
//! server's reason; the row stays, since nothing was deleted. A delete that
//! succeeds needs nothing here: the watch drops the row when the pod goes.

use super::*;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::delete_flow::refusal::{self, Refusal};
use crate::k8s::resource::delete_flow::{self, DeleteTarget};

/// The failure banner, and its Dismiss button.
pub(super) const FAILURE_ID: &str = "pods-action-failure";
pub(super) const DISMISS_FAILURE_ID: &str = "pods-action-failure-dismiss";
/// The notice an action leaves, like where a port-forward listens.
pub(super) const NOTICE_ID: &str = "pods-action-notice";

/// An action the cluster refused, as the banner above the table shows it.
pub(super) type PodActionFailure = Refusal;

impl PodsPanel {
    /// `DeletePod`: asks before deleting the selected pod.
    pub(super) fn on_action_delete_pod(
        &mut self,
        _: &DeletePod,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.delete_target(cx) else {
            return;
        };
        self.clear_action_report(cx);
        let on_done = self.on_deleted(&target, false, cx);
        let connection = self.connection.clone();
        delete_flow::confirm_delete(target, connection, on_done, window, cx);
    }

    /// `KillPod`: deletes the selected pod at once, with no grace period and
    /// no prompt.
    pub(super) fn on_action_kill_pod(
        &mut self,
        _: &KillPod,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.delete_target(cx) else {
            return;
        };
        self.clear_action_report(cx);
        let on_done = self.on_deleted(&target, true, cx);
        let connection = self.connection.clone();
        delete_flow::send_delete(target, &connection, true, on_done, cx);
    }

    /// The selected pod, as a delete addresses it.
    fn delete_target(&self, cx: &App) -> Option<DeleteTarget> {
        let selection = self.table_selection(cx)?;
        Some(DeleteTarget {
            context_name: selection.context_name,
            kind: DiscoveredKind::pods(),
            namespace: Some(selection.namespace),
            name: selection.name,
        })
    }

    fn clear_action_report(&mut self, cx: &mut Context<Self>) {
        self.action_failure = None;
        self.action_notice = None;
        cx.notify();
    }

    /// Shows a refused delete of `target` as the banner; a delete that lands
    /// needs nothing, since the watch drops the row.
    fn on_deleted(
        &self,
        target: &DeleteTarget,
        force: bool,
        cx: &mut Context<Self>,
    ) -> delete_flow::OnDeleted {
        let panel = cx.weak_entity();
        let action = target.action(force);
        std::rc::Rc::new(move |result, cx| {
            let _ = panel.update(cx, |panel, cx| {
                panel.action_failure = result.err().map(|failure| Refusal {
                    action: action.clone(),
                    failure,
                });
                cx.notify();
            });
        })
    }

    /// What the last action did, when it says something, above the table.
    pub(super) fn render_action_notice(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let notice = self.action_notice.clone()?;
        let space = crate::ui::space::spacing(cx);
        Some(
            div()
                .debug_selector(|| NOTICE_ID.into())
                .mb(space.control_gap)
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(notice)
                .into_any_element(),
        )
    }

    /// The banner for a refused action, above the table.
    pub(super) fn render_action_failure(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let refusal = self.action_failure.clone()?;
        let this = cx.weak_entity();
        Some(refusal::render(
            refusal,
            FAILURE_ID,
            DISMISS_FAILURE_ID,
            move |cx| {
                let _ = this.update(cx, |this, cx| {
                    this.action_failure = None;
                    cx.notify();
                });
            },
            cx,
        ))
    }
}

#[cfg(test)]
mod tests;
