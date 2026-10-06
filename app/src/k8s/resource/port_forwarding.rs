//! Port-forward from a resource row (`k9s-remaining-keybindings` 4), the parts
//! the Pods and Services panels share: which ports a resource offers, asking
//! which one when there are several, resolving a Service port to a Pod port -
//! Kubernetes forwards to Pods only, so a Service's goes to one of its Running
//! Pods, as `kubectl port-forward svc/...` does - and starting the forward in
//! the app's list (`k8s::cluster::port_forwards`), where Manage Tunnels shows it.

use crate::k8s::cluster::port_forwards::{ForwardObject, PortForwardRequest, PortForwards};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::*;
use k8s_openapi::api::core::v1::{Pod, Service};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::Api;
use kube::api::ListParams;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::rc::Rc;

/// One port a resource offers, and how the picker names it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PortChoice {
    pub(crate) port: u16,
    pub(crate) label: String,
}

/// The picker's Cancel, and each port's button.
pub(crate) const CANCEL_PORT_ID: &str = "port-forward-cancel";
pub(crate) fn port_button_id(port: u16) -> SharedString {
    format!("port-forward-port-{port}").into()
}

/// `pod`'s declared container ports, each once, in spec order.
pub(crate) fn pod_ports(pod: &Pod) -> Vec<PortChoice> {
    let mut ports: Vec<PortChoice> = Vec::new();
    for container in pod.spec.iter().flat_map(|spec| &spec.containers) {
        for declared in container.ports.iter().flatten() {
            let Ok(port) = u16::try_from(declared.container_port) else {
                continue;
            };
            if ports.iter().any(|choice| choice.port == port) {
                continue;
            }
            let label = match &declared.name {
                Some(name) => format!("{port} ({name}, {})", container.name),
                None => format!("{port} ({})", container.name),
            };
            ports.push(PortChoice { port, label });
        }
    }
    ports
}

/// `service`'s ports, each once.
pub(crate) fn service_ports(service: &Service) -> Vec<PortChoice> {
    service
        .spec
        .iter()
        .flat_map(|spec| spec.ports.iter().flatten())
        .filter_map(|port| {
            let number = u16::try_from(port.port).ok()?;
            Some(PortChoice {
                port: number,
                label: match &port.name {
                    Some(name) => format!("{number} ({name})"),
                    None => number.to_string(),
                },
            })
        })
        .collect()
}

/// Whether `labels` carry every pair `selector` asks for. An empty selector
/// matches nothing: a Service without one has no Pods of its own.
pub(crate) fn matches_selector(
    labels: Option<&BTreeMap<String, String>>,
    selector: &BTreeMap<String, String>,
) -> bool {
    !selector.is_empty()
        && selector
            .iter()
            .all(|(key, value)| labels.and_then(|labels| labels.get(key)) == Some(value))
}

/// The Pod and Pod port a forward of `service`'s `service_port` goes to: one
/// of the Pods it selects that is Running, and the port its `targetPort` names
/// there - by number, or by the container port's name.
pub(crate) async fn resolve_service(
    client: kube::Client,
    namespace: &str,
    service: &str,
    service_port: u16,
) -> Result<(String, u16), String> {
    let services: Api<Service> = Api::namespaced(client.clone(), namespace);
    let service = services
        .get(service)
        .await
        .map_err(|error| crate::k8s::error::describe(&error))?;
    let spec = service.spec.unwrap_or_default();
    let port = spec
        .ports
        .iter()
        .flatten()
        .find(|port| port.port == i32::from(service_port))
        .ok_or_else(|| format!("The service has no port {service_port}."))?;
    let selector = spec.selector.unwrap_or_default();
    let pods: Api<Pod> = Api::namespaced(client, namespace);
    let listed = pods
        .list(&ListParams::default())
        .await
        .map_err(|error| crate::k8s::error::describe(&error))?;
    let pod = listed
        .items
        .into_iter()
        .filter(|pod| matches_selector(pod.metadata.labels.as_ref(), &selector))
        .find(|pod| pod.status.as_ref().and_then(|s| s.phase.as_deref()) == Some("Running"))
        .ok_or_else(|| "None of the service's pods is Running.".to_string())?;
    let target = match &port.target_port {
        None => Some(service_port),
        Some(IntOrString::Int(number)) => u16::try_from(*number).ok(),
        Some(IntOrString::String(name)) => pod
            .spec
            .iter()
            .flat_map(|spec| &spec.containers)
            .flat_map(|container| container.ports.iter().flatten())
            .find(|declared| declared.name.as_deref() == Some(name.as_str()))
            .and_then(|declared| u16::try_from(declared.container_port).ok()),
    }
    .ok_or_else(|| "The service's target port isn't one its pod declares.".to_string())?;
    Ok((pod.metadata.name.unwrap_or_default(), target))
}

/// Starts forwarding `request` in the app's list, through `context_name`'s
/// connection when it is up.
pub(crate) fn start(
    request: PortForwardRequest,
    origin: ForwardObject,
    cx: &mut App,
) -> Result<SocketAddr, String> {
    let connection =
        crate::k8s::cluster::session::ClusterRegistry::connection(cx, &request.context_name);
    let crate::k8s::cluster::connection::ConnectionState::Connected(client) =
        connection.read(cx).state.clone()
    else {
        return Err("The cluster isn't connected.".into());
    };
    PortForwards::entity(cx).update(cx, |forwards, cx| {
        forwards.start(request, origin, client, cx)
    })
}

/// Reports a forward that didn't start as a transient notification naming the
/// target, the port when one was chosen, and why (`port-forward-indicators`
/// 2.2). A started one needs no report: its row's Forwards cell shows it.
pub(crate) fn notify_failure(
    target: &str,
    port: Option<u16>,
    reason: &str,
    window: &mut Window,
    cx: &mut App,
) {
    use gpui_kit::component::WindowExt as _;
    use gpui_kit::component::notification::Notification;
    let title = match port {
        Some(port) => format!("Couldn't forward {target}:{port}"),
        None => format!("Couldn't forward {target}"),
    };
    #[cfg(test)]
    cx.default_global::<NotifiedFailures>()
        .0
        .push((title.clone(), reason.to_string()));
    window.push_notification(Notification::error(reason.to_string()).title(title), cx);
}

/// Test-only record of every failure notification: gpui-component keeps a
/// window's notification list private, so tests read what was pushed here.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct NotifiedFailures(pub(crate) Vec<(String, String)>);

#[cfg(test)]
impl gpui_kit::Global for NotifiedFailures {}

/// Asks which of `choices` to forward, then calls `chosen` with it. Keyboard:
/// Tab to a port, Space or Enter; Escape or Cancel to back out.
pub(crate) fn ask_port(
    title: &'static str,
    choices: Vec<PortChoice>,
    chosen: impl Fn(u16, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let chosen = Rc::new(chosen);
    window.open_dialog(cx, move |dialog, _window, _cx| {
        let mut footer = DialogFooter::new();
        for choice in &choices {
            let (chosen, port) = (chosen.clone(), choice.port);
            footer = footer.child(
                Button::new(port_button_id(port))
                    .label(choice.label.clone())
                    .primary()
                    .on_click(move |_event, window, cx| {
                        window.close_dialog(cx);
                        chosen(port, window, cx);
                    }),
            );
        }
        dialog
            .title(title)
            .child("It offers more than one port.")
            .footer(
                footer.child(Button::new(CANCEL_PORT_ID).label("Cancel").on_click(
                    |_event, window, cx| {
                        window.close_dialog(cx);
                    },
                )),
            )
    });
}

#[cfg(test)]
mod tests;
