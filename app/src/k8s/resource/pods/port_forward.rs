//! Port-forward the selected pod (`k9s-remaining-keybindings` 4.1, 4.2):
//! `shift-f` forwards its declared port - asking which, when it declares
//! several - into the app's forwards, which Manage Tunnels lists and stops.
//! [`forward_pod`] is the flow itself, which the pod detail panel shares.

use super::actions::PodActionFailure;
use super::*;
use crate::k8s::cluster::port_forwards::PortForwardRequest;
use crate::k8s::resource::port_forwarding::{self, PortChoice};
use crate::k8s::resource::resource_actions::ActionFailure;
use std::rc::Rc;

/// How a pod's port-forward went: where it listens, or why it didn't start.
pub(crate) type ForwardReport = Rc<dyn Fn(Result<String, String>, &mut App)>;

/// Forwards `pod`'s declared port - asking which first when it declares more
/// than one - and tells `report` where it listens, or why it didn't start.
/// Shared by the Pods list and the pod detail panel, so both ask alike.
///
/// `report` always runs deferred, never inside this call: a panel calls this
/// from its own action handler, while it is being updated, and `report`
/// updates it.
pub(crate) fn forward_pod(
    selection: PodSelection,
    pod: Option<&Pod>,
    report: ForwardReport,
    window: &mut Window,
    cx: &mut App,
) {
    let ports: Vec<PortChoice> = pod.map(port_forwarding::pod_ports).unwrap_or_default();
    match ports.as_slice() {
        [] => {
            let message = format!("Pod {} declares no container ports.", selection.name);
            cx.defer(move |cx| report(Err(message), cx));
        }
        [only] => {
            let port = only.port;
            forward(&selection, port, &report, cx);
        }
        _ => port_forwarding::ask_port(
            "Forward Which Port?",
            ports,
            move |port, _window, cx| forward(&selection, port, &report, cx),
            window,
            cx,
        ),
    }
}

fn forward(selection: &PodSelection, port: u16, report: &ForwardReport, cx: &mut App) {
    let request = PortForwardRequest {
        context_name: selection.context_name.clone(),
        namespace: selection.namespace.clone(),
        pod: selection.name.clone(),
        remote_port: port,
    };
    let result = port_forwarding::start(request, cx)
        .map(|addr| port_forwarding::started_notice(addr, &selection.name, port));
    let report = report.clone();
    cx.defer(move |cx| report(result, cx));
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
        let panel = cx.weak_entity();
        let action = format!("Port-forward pod {}", selection.name);
        let report: ForwardReport = Rc::new(move |result, cx| {
            let _ = panel.update(cx, |panel, cx| panel.report_forward(&action, result, cx));
        });
        let pod = self
            .table
            .read(cx)
            .find(&selection.namespace, &selection.name, None)
            .cloned();
        forward_pod(selection, pod.as_ref(), report, window, cx);
    }

    /// Shows how a forward went: where it listens, or why it didn't start.
    fn report_forward(
        &mut self,
        action: &str,
        result: Result<String, String>,
        cx: &mut Context<Self>,
    ) {
        self.action_failure = None;
        self.action_notice = None;
        match result {
            Ok(notice) => self.action_notice = Some(notice),
            Err(message) => {
                self.action_failure = Some(PodActionFailure {
                    action: action.to_string(),
                    failure: ActionFailure {
                        message,
                        detail: String::new(),
                    },
                });
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
