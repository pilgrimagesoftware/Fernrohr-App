//! Port-forward the selected pod (`k9s-remaining-keybindings` 4.1, 4.2):
//! `shift-f` forwards its declared port - asking which, when it declares
//! several - into the app's forwards, which Manage Tunnels lists and stops.

use super::actions::PodActionFailure;
use super::*;
use crate::k8s::cluster::port_forwards::PortForwardRequest;
use crate::k8s::resource::port_forwarding::{self, PortChoice};
use crate::k8s::resource::resource_actions::ActionFailure;

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
        let ports: Vec<PortChoice> = self
            .table
            .read(cx)
            .find(&selection.namespace, &selection.name, None)
            .map(port_forwarding::pod_ports)
            .unwrap_or_default();
        match ports.as_slice() {
            [] => self.report_forward(
                &selection,
                Err(format!(
                    "Pod {} declares no container ports.",
                    selection.name
                )),
                None,
                cx,
            ),
            [only] => {
                let port = only.port;
                self.forward(&selection, port, cx);
            }
            _ => {
                let panel = cx.weak_entity();
                port_forwarding::ask_port(
                    "Forward Which Port?",
                    ports,
                    move |port, _window, cx| {
                        let _ = panel.update(cx, |panel, cx| panel.forward(&selection, port, cx));
                    },
                    window,
                    cx,
                );
            }
        }
    }

    fn forward(&mut self, selection: &PodSelection, port: u16, cx: &mut Context<Self>) {
        let request = PortForwardRequest {
            context_name: selection.context_name.clone(),
            namespace: selection.namespace.clone(),
            pod: selection.name.clone(),
            remote_port: port,
        };
        let result = port_forwarding::start(request, cx);
        self.report_forward(selection, result.map(Some), Some(port), cx);
    }

    /// Shows how a forward went: where it listens, or why it didn't start.
    fn report_forward(
        &mut self,
        selection: &PodSelection,
        result: Result<Option<std::net::SocketAddr>, String>,
        port: Option<u16>,
        cx: &mut Context<Self>,
    ) {
        self.action_failure = None;
        self.action_notice = None;
        match (result, port) {
            (Ok(Some(addr)), Some(port)) => {
                self.action_notice =
                    Some(port_forwarding::started_notice(addr, &selection.name, port));
            }
            (Ok(_), _) => {}
            (Err(message), _) => {
                self.action_failure = Some(PodActionFailure {
                    action: format!("Port-forward pod {}", selection.name),
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
