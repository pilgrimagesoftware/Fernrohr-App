//! Port-forward the selected Service (`k9s-remaining-keybindings` 4.1, 4.2):
//! in a Services list, `shift-f` forwards one of the Service's ports - asking
//! which when it has several - to a Running Pod behind it, into the app's
//! forwards that Manage Tunnels lists and stops.
//!
//! Offered only in a list of core Services: the table sits in a
//! [`SERVICES_KEY_CONTEXT`] element there, which is the command's context.

use super::commands::PortForwardService;
use super::panel::ObjectListPanel;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::port_forwards::{ForwardObject, PortForwardRequest};
use crate::k8s::resource::port_forwarding::{self, PortChoice};
use gpui_kit::*;
use k8s_openapi::api::core::v1::Service;
use kube::Api;

/// The context the Services port-forward command lives in.
pub const SERVICES_KEY_CONTEXT: &str = "ServicesList";
impl ObjectListPanel {
    /// Whether this panel lists core Services.
    pub(super) fn lists_services(&self) -> bool {
        self.kind.gvk.group.is_empty() && self.kind.gvk.kind == "Service"
    }

    /// The forwards started from `row`'s Service; none in any other list.
    pub(super) fn forwards_of(
        &self,
        row: &super::row::ObjectRow,
        cx: &App,
    ) -> Vec<crate::k8s::cluster::port_forwards::ForwardSummary> {
        let (true, Some(namespace), Some(forwards)) = (
            self.lists_services(),
            row.namespace.as_deref(),
            crate::k8s::cluster::port_forwards::PortForwards::existing(cx),
        ) else {
            return Vec::new();
        };
        let service = ForwardObject::service(&self.scope.context_name, namespace, &row.name);
        forwards.read(cx).for_object(&service)
    }

    /// The selected row's namespace and name.
    fn selected_object(&self, cx: &App) -> Option<(String, String)> {
        let row_ix = self.selected_row(cx)?;
        let row = self
            .table
            .as_ref()?
            .read(cx)
            .delegate()
            .rows()
            .get(row_ix)?
            .clone();
        Some((row.object.namespace.clone()?, row.object.name.clone()))
    }

    /// `PortForwardService`: reads the Service's ports, asks which when there
    /// are several, and forwards it to a Running Pod behind it.
    pub(super) fn on_action_port_forward_service(
        &mut self,
        _: &PortForwardService,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.lists_services() {
            return;
        }
        let Some((namespace, name)) = self.selected_object(cx) else {
            return;
        };
        let ConnectionState::Connected(client) = &self.connection.read(cx).state else {
            return;
        };
        let client = client.clone();
        let rx = crate::runtime::spawn_stream(cx, 1, {
            let (namespace, name) = (namespace.clone(), name.clone());
            move |tx| async move {
                let services: Api<Service> = Api::namespaced(client, &namespace);
                let ports = services
                    .get(&name)
                    .await
                    .map(|service| port_forwarding::service_ports(&service))
                    .map_err(|error| crate::k8s::error::describe(&error));
                let _ = tx.send(ports).await;
            }
        });
        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, |ports| {
                let _ = window_handle.update(cx, |_, window, cx| {
                    let _ = this.update(cx, |this, cx| {
                        this.choose_service_port(namespace.clone(), name.clone(), ports, window, cx)
                    });
                });
            })
            .await;
        })
        .detach();
    }

    fn choose_service_port(
        &mut self,
        namespace: String,
        name: String,
        ports: Result<Vec<PortChoice>, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ports = match ports {
            Ok(ports) => ports,
            Err(reason) => {
                return port_forwarding::notify_failure(&name, None, &reason, window, cx);
            }
        };
        match ports.as_slice() {
            [] => {
                let reason = format!("Service {name} has no ports.");
                port_forwarding::notify_failure(&name, None, &reason, window, cx);
            }
            [only] => {
                let port = only.port;
                self.forward_service(namespace, name, port, window, cx);
            }
            _ => {
                let panel = cx.weak_entity();
                port_forwarding::ask_port(
                    "Forward Which Port?",
                    ports,
                    move |port, window, cx| {
                        let _ = panel.update(cx, |panel, cx| {
                            panel.forward_service(namespace.clone(), name.clone(), port, window, cx)
                        });
                    },
                    window,
                    cx,
                );
            }
        }
    }

    /// Resolves `port` of Service `name` to a Running Pod's port, and forwards it.
    /// Success shows in the Service's Forwards cell; a failure is a notification.
    fn forward_service(
        &mut self,
        namespace: String,
        name: String,
        port: u16,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ConnectionState::Connected(client) = &self.connection.read(cx).state else {
            return;
        };
        let client = client.clone();
        let context_name = self.scope.context_name.clone();
        let rx = crate::runtime::spawn_stream(cx, 1, {
            let (namespace, name) = (namespace.clone(), name.clone());
            move |tx| async move {
                let _ = tx
                    .send(port_forwarding::resolve_service(client, &namespace, &name, port).await)
                    .await;
            }
        });
        let window_handle = window.window_handle();
        cx.spawn(async move |_this, cx| {
            crate::runtime::drain(rx, |resolved| {
                let _ = window_handle.update(cx, |_, window, cx| {
                    let result = resolved.and_then(|(pod, pod_port)| {
                        let request = PortForwardRequest {
                            context_name: context_name.clone(),
                            namespace: namespace.clone(),
                            pod,
                            remote_port: pod_port,
                        };
                        let origin = ForwardObject::service(&context_name, &namespace, &name);
                        port_forwarding::start(request, origin, cx)
                    });
                    if let Err(reason) = result {
                        port_forwarding::notify_failure(&name, Some(port), &reason, window, cx);
                    }
                });
            })
            .await;
        })
        .detach();
    }
}

#[cfg(test)]
mod tests;
