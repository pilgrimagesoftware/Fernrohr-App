//! Port-forward the selected pod (`k9s-remaining-keybindings` 4.1, 4.2):
//! `shift-f` forwards its declared port - asking which, when it declares
//! several - into the app's forwards, which Manage Tunnels lists and stops.
//! [`forward_pod`] is the flow itself, which the pod detail panel shares. A
//! started forward shows where the pod is shown - its row's Forwards cell, its
//! detail panel's strip - and one that can't start is a notification
//! (`port-forward-indicators` 2.2).

use super::*;
use crate::k8s::cluster::port_forwards::{ForwardObject, PortForwardRequest};
use crate::k8s::resource::port_forwarding::{self, PortChoice};

/// Forwards `pod`'s declared port - asking which first when it declares more
/// than one - or says in a notification why it can't. Shared by the Pods list
/// and the pod detail panel, so both ask alike.
pub(crate) fn forward_pod(
    selection: PodSelection,
    pod: Option<&Pod>,
    window: &mut Window,
    cx: &mut App,
) {
    let ports: Vec<PortChoice> = pod.map(port_forwarding::pod_ports).unwrap_or_default();
    match ports.as_slice() {
        [] => {
            let reason = format!("Pod {} declares no container ports.", selection.name);
            port_forwarding::notify_failure(&selection.name, None, &reason, window, cx);
        }
        [only] => {
            let port = only.port;
            forward_port(&selection, port, window, cx);
        }
        _ => port_forwarding::ask_port(
            "Forward Which Port?",
            ports,
            move |port, window, cx| forward_port(&selection, port, window, cx),
            window,
            cx,
        ),
    }
}

/// Starts forwarding `selection`'s `port`, with no prompt - what a container
/// port's start button does too - or says in a notification why it can't.
pub(crate) fn forward_port(selection: &PodSelection, port: u16, window: &mut Window, cx: &mut App) {
    let request = PortForwardRequest {
        context_name: selection.context_name.clone(),
        namespace: selection.namespace.clone(),
        pod: selection.name.clone(),
        remote_port: port,
    };
    let origin = ForwardObject::pod(
        &selection.context_name,
        &selection.namespace,
        &selection.name,
    );
    if let Err(reason) = port_forwarding::start(request, origin, cx) {
        port_forwarding::notify_failure(&selection.name, Some(port), &reason, window, cx);
    }
}

impl PodsPanel {
    /// `PortForwardPod`: forwards the selected pod's port, asking which first
    /// when it declares more than one.
    pub(super) fn on_action_port_forward_pod(
        &mut self,
        _: &PortForwardPod,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.table_selection(cx) else {
            return;
        };
        let pod = self
            .table
            .read(cx)
            .find(&selection.namespace, &selection.name, None)
            .cloned();
        forward_pod(selection, pod.as_ref(), window, cx);
    }

    /// The selected pod's forwards (`port-forward-indicators`).
    pub(super) fn selected_forwards(
        &self,
        cx: &App,
    ) -> Vec<crate::k8s::cluster::port_forwards::ForwardSummary> {
        let (Some(selection), Some(forwards)) = (
            self.table_selection(cx),
            crate::k8s::cluster::port_forwards::PortForwards::existing(cx),
        ) else {
            return Vec::new();
        };
        forwards.read(cx).for_object(&ForwardObject::pod(
            &selection.context_name,
            &selection.namespace,
            &selection.name,
        ))
    }

    /// `StopPortForward`: stops one of the selected pod's forwards - asking which
    /// when it has several - once confirmed.
    pub(super) fn on_action_stop_port_forward(
        &mut self,
        _: &StopPortForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.table_selection(cx) else {
            return;
        };
        let forwards = self.selected_forwards(cx);
        crate::ui::forward_stop::stop_one_of("pod", selection.name, forwards, window, cx);
    }
}

#[cfg(test)]
mod tests;
