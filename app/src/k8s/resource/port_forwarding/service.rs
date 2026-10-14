//! Port-forwarding a Service by name (`k9s-remaining-keybindings` 4.1, 4.2):
//! read its ports, ask which when it has several, resolve the chosen one to a
//! Running Pod behind it, and forward to that Pod. The one flow behind every
//! entry point - a Services list's `shift-f` and row menu, and a Service's
//! detail panel (`resource-specific-actions`).

use super::{PortChoice, ask_port, notify_failure, resolve_service, service_ports, start};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::port_forwards::{ForwardObject, PortForwardRequest};
use gpui_kit::*;
use k8s_openapi::api::core::v1::Service;
use kube::Api;

/// Forwards Service `namespace/name` on `context_name`, over `connection`.
/// Success shows wherever forwards are listed; a failure is a notification.
pub(crate) fn forward_service(
    context_name: String,
    connection: &Entity<ClusterConnection>,
    namespace: String,
    name: String,
    window: &mut Window,
    cx: &mut App,
) {
    let ConnectionState::Connected(client) = &connection.read(cx).state else {
        let reason = format!("{context_name} is not connected.");
        return notify_failure(&name, None, &reason, window, cx);
    };
    let client = client.clone();
    let rx = crate::runtime::spawn_stream(cx, 1, {
        let (client, namespace, name) = (client.clone(), namespace.clone(), name.clone());
        move |tx| async move {
            let ports = Api::<Service>::namespaced(client, &namespace)
                .get(&name)
                .await
                .map(|service| service_ports(&service))
                .map_err(|error| crate::k8s::error::describe(&error));
            let _ = tx.send(ports).await;
        }
    });
    let window_handle = window.window_handle();
    cx.spawn(async move |cx| {
        crate::runtime::drain(rx, |ports| {
            let target = Target {
                context_name: context_name.clone(),
                client: client.clone(),
                namespace: namespace.clone(),
                name: name.clone(),
            };
            let _ = window_handle.update(cx, |_, window, cx| choose(target, ports, window, cx));
        })
        .await;
    })
    .detach();
}

/// The Service being forwarded.
#[derive(Clone)]
struct Target {
    context_name: String,
    client: kube::Client,
    namespace: String,
    name: String,
}

fn choose(
    target: Target,
    ports: Result<Vec<PortChoice>, String>,
    window: &mut Window,
    cx: &mut App,
) {
    let ports = match ports {
        Ok(ports) => ports,
        Err(reason) => return notify_failure(&target.name, None, &reason, window, cx),
    };
    match ports.as_slice() {
        [] => {
            let reason = format!("Service {} has no ports.", target.name);
            notify_failure(&target.name, None, &reason, window, cx);
        }
        [only] => forward(target, only.port, window, cx),
        _ => ask_port(
            "Forward Which Port?",
            ports,
            move |port, window, cx| forward(target.clone(), port, window, cx),
            window,
            cx,
        ),
    }
}

/// Resolves `port` of the Service to a Running Pod's port, and forwards it.
fn forward(target: Target, port: u16, window: &mut Window, cx: &mut App) {
    let rx = crate::runtime::spawn_stream(cx, 1, {
        let target = target.clone();
        move |tx| async move {
            let resolved =
                resolve_service(target.client, &target.namespace, &target.name, port).await;
            let _ = tx.send(resolved).await;
        }
    });
    let window_handle = window.window_handle();
    cx.spawn(async move |cx| {
        crate::runtime::drain(rx, |resolved| {
            let _ = window_handle.update(cx, |_, window, cx| {
                let Target {
                    context_name,
                    namespace,
                    name,
                    ..
                } = &target;
                let result = resolved.and_then(|(pod, pod_port)| {
                    let request = PortForwardRequest {
                        context_name: context_name.clone(),
                        namespace: namespace.clone(),
                        pod,
                        remote_port: pod_port,
                    };
                    let origin = ForwardObject::service(context_name, namespace, name);
                    start(request, origin, cx)
                });
                if let Err(reason) = result {
                    notify_failure(name, Some(port), &reason, window, cx);
                }
            });
        })
        .await;
    })
    .detach();
}
