//! Acting on the panel's own pod (`k9s-remaining-keybindings`), with the Pods
//! list's actions and flows: Delete (`ctrl-d`, asking first), Kill (`ctrl-k`, at
//! once), Shell (`s`, only while a container runs) and Port forward
//! (`shift-f`). Each reuses the list's action and its shared flow -
//! `delete_flow`, `pods::shell_into`, `pods::forward_pod` - so they ask, key
//! and behave alike from either place.
//!
//! A delete or kill that lands needs nothing here: the panel follows its pod
//! (`live`), so it shows it Terminating, then deleted. A refused action shows
//! why above the content; a forward says where it listens.

use super::commands::{DELETE_KEY, PANEL_KEY_CONTEXT, PORT_FORWARD_KEY, SHELL_KEY};
use super::fetch::PodDetailState;
use super::panel::PodDetailPanel;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::delete_flow::refusal::{self, Refusal};
use crate::k8s::resource::delete_flow::{self, DeleteTarget};
use crate::k8s::resource::pods::{
    DeletePod, KillPod, PodSelection, PortForwardPod, ShellPod, forward_pod, running_containers,
    shell_into,
};
use crate::ui::detail::lifecycle::Lifecycle;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;
use std::rc::Rc;

/// Added beside the panel's own context while its pod has a running
/// container - the Shell command's context.
pub const SHELLABLE_KEY_CONTEXT: &str = "PodDetailShellable";
/// Added while the pod can be deleted - Delete's and Kill's context.
pub const DELETABLE_KEY_CONTEXT: &str = "DeletablePod";
/// The debug selector of the hint labelled `label`.
pub(super) fn hint_selector(label: &str) -> String {
    format!("pod-detail-hint {label}")
}

/// The refusal banner, its Dismiss button, and a forward's notice.
pub(super) const REFUSAL_ID: &str = "pod-detail-refusal";
pub(super) const DISMISS_REFUSAL_ID: &str = "pod-detail-refusal-dismiss";

impl PodDetailPanel {
    /// The pod, while there is one to act on: loaded and not known gone.
    fn live_pod(&self) -> Option<&k8s_openapi::api::core::v1::Pod> {
        let gone = matches!(self.notice, Some(Lifecycle::Deleted { .. }));
        match &self.state {
            PodDetailState::Loaded(pod) if !gone => Some(pod),
            _ => None,
        }
    }

    /// Whether Delete and Kill are offered: there is a live pod, and the Pod
    /// kind can be deleted - by discovery's word when it has one, else assumed.
    pub(super) fn deletable(&self, cx: &App) -> bool {
        let pods = DiscoveredKind::pods();
        let verbs = self
            .discovery
            .read(cx)
            .kinds()
            .and_then(|kinds| kinds.iter().find(|kind| **kind == pods))
            .map_or(pods.verbs, |kind| kind.verbs);
        self.live_pod().is_some() && verbs.delete
    }

    /// The pod's running containers - empty when there is no live pod.
    pub(super) fn running(&self) -> Vec<String> {
        self.live_pod().map(running_containers).unwrap_or_default()
    }

    /// The pod as the Pods list's actions address it.
    pub(super) fn selection(&self) -> PodSelection {
        let containers = self
            .live_pod()
            .and_then(|pod| pod.spec.as_ref())
            .map(|spec| spec.containers.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default();
        PodSelection {
            namespace: self.pod.namespace.clone(),
            name: self.pod.name.clone(),
            containers,
            context_name: self.scope.context_name.clone(),
        }
    }

    fn delete_target(&self) -> DeleteTarget {
        DeleteTarget {
            context_name: self.scope.context_name.clone(),
            kind: DiscoveredKind::pods(),
            namespace: Some(self.pod.namespace.clone()),
            name: self.pod.name.clone(),
        }
    }

    fn clear_action_report(&mut self, cx: &mut Context<Self>) {
        self.action_refusal = None;
        cx.notify();
    }

    /// Shows a refused delete or kill as the banner.
    fn on_deleted(&self, force: bool, cx: &mut Context<Self>) -> delete_flow::OnDeleted {
        let panel = cx.weak_entity();
        let action = self.delete_target().action(force);
        Rc::new(move |result, cx| {
            let _ = panel.update(cx, |panel, cx| {
                panel.action_refusal = result.err().map(|failure| Refusal {
                    action: action.clone(),
                    failure,
                });
                cx.notify();
            });
        })
    }

    /// `DeletePod`: asks before deleting the panel's pod.
    pub(super) fn on_action_delete_pod(
        &mut self,
        _: &DeletePod,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.deletable(cx) {
            return;
        }
        self.clear_action_report(cx);
        let on_done = self.on_deleted(false, cx);
        let connection = self.connection.clone();
        delete_flow::confirm_delete(self.delete_target(), connection, on_done, window, cx);
    }

    /// `KillPod`: deletes the panel's pod at once, with no grace period.
    pub(super) fn on_action_kill_pod(
        &mut self,
        _: &KillPod,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.deletable(cx) {
            return;
        }
        self.clear_action_report(cx);
        let on_done = self.on_deleted(true, cx);
        let connection = self.connection.clone();
        delete_flow::send_delete(self.delete_target(), &connection, true, on_done, cx);
    }

    /// `ShellPod`: a shell in the pod's running container, asking which when
    /// it runs several.
    pub(super) fn on_action_shell_pod(
        &mut self,
        _: &ShellPod,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let running = self.running();
        if running.is_empty() {
            return;
        }
        shell_into(self.selection(), running, window, cx);
    }

    /// `PortForwardPod`: forwards the pod's port, asking which when it
    /// declares several. The panel's forward strip shows it started; a failure
    /// is a notification.
    pub(super) fn on_action_port_forward_pod(
        &mut self,
        _: &PortForwardPod,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pod) = self.live_pod().cloned() else {
            return;
        };
        self.clear_action_report(cx);
        forward_pod(self.selection(), Some(&pod), window, cx);
    }

    /// The acting keys' hints, after Logs: Shell while `shellable`, Port
    /// forward while there is a pod to act on, Delete while it can be deleted.
    pub(super) fn acting_hints(&self, shellable: bool, window: &Window, cx: &App) -> Vec<Div> {
        let live = self.live_pod().is_some();
        let deletable = self.deletable(cx);
        let hint = |action: &dyn Action, context: &str, fallback: &str, label: &'static str| {
            let key = Kbd::binding_for_action(action, Some(context), window)
                .unwrap_or_else(|| Kbd::new(Keystroke::parse(fallback).expect("valid keybinding")));
            let selector = hint_selector(label);
            div()
                .debug_selector(move || selector)
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap_1()
                .whitespace_nowrap()
                .child(key)
                .child(label)
        };
        let mut hints = Vec::new();
        if shellable {
            hints.push(hint(&ShellPod, SHELLABLE_KEY_CONTEXT, SHELL_KEY, "Shell"));
        }
        if live {
            hints.push(hint(
                &PortForwardPod,
                PANEL_KEY_CONTEXT,
                PORT_FORWARD_KEY,
                "Port forward",
            ));
        }
        if !self.pod_forwards(cx).is_empty() {
            hints.push(hint(
                &crate::k8s::resource::pods::StopPortForward,
                PANEL_KEY_CONTEXT,
                crate::k8s::resource::pods::STOP_PORT_FORWARD_KEY,
                "Stop forward",
            ));
        }
        if deletable {
            hints.push(hint(
                &DeletePod,
                DELETABLE_KEY_CONTEXT,
                DELETE_KEY,
                "Delete",
            ));
        }
        hints
    }

    /// What the last action left above the content: a refusal, or a notice.
    pub(super) fn render_action_report(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let space = crate::ui::space::spacing(cx);
        let wrap = |element: AnyElement| {
            div()
                .px(space.panel_inset)
                .pt(space.control_gap)
                .child(element)
                .into_any_element()
        };
        let refused = self.action_refusal.clone()?;
        let this = cx.weak_entity();
        Some(wrap(refusal::render(
            refused,
            REFUSAL_ID,
            DISMISS_REFUSAL_ID,
            move |cx| {
                let _ = this.update(cx, |this, cx| {
                    this.action_refusal = None;
                    cx.notify();
                });
            },
            cx,
        )))
    }
}
