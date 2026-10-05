//! Port-forwards started from a resource row (`k9s-remaining-keybindings` 4),
//! app-wide: the one list the Pods and Services panels add to and the Tunnels
//! window shows and stops.
//!
//! Each forward is a [`K8sPortForward`] shared through a [`ForwardRegistry`]
//! keyed by its [`PortForwardRequest`]; this list holds the one handle that
//! keeps it alive. Stopping drops that handle, which releases the forward
//! through the registry's reference-counted shutdown - the listener closes and
//! its key leaves the live set. Starting the same forward twice is a no-op that
//! returns the running one.

use crate::forward::k8s::managed::K8sPortForward;
use crate::forward::k8s::port_forward::K8sPortForwardConfig;
use crate::forward::managed::{ForwardState, ManagedForward};
use crate::forward::registry::{ForwardRegistry, RegistryHandle};
use crate::forward::supervisor::{BackoffPolicy, SupervisorOptions};
use gpui_kit::*;
use std::collections::BTreeMap;
use std::net::SocketAddr;

/// What to forward: one Pod port, on one context's cluster. Also the forward's
/// identity - the same request is the same forward.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PortForwardRequest {
    pub context_name: String,
    pub namespace: String,
    pub pod: String,
    pub remote_port: u16,
}

/// One running forward: the handle that keeps it alive, where it listens, and
/// its state as last reported.
struct Held {
    handle: RegistryHandle<PortForwardRequest, K8sPortForward>,
    state: ForwardState,
    _follow: Task<()>,
}

/// The app's row-started port-forwards.
#[derive(Default)]
pub struct PortForwards {
    registry: ForwardRegistry<PortForwardRequest, K8sPortForward>,
    held: BTreeMap<PortForwardRequest, Held>,
}

/// The app-scoped handle to the one [`PortForwards`].
struct PortForwardsGlobal(Entity<PortForwards>);

impl Global for PortForwardsGlobal {}

impl PortForwards {
    /// The app's forwards, created on first use.
    pub fn entity(cx: &mut App) -> Entity<Self> {
        if let Some(global) = cx.try_global::<PortForwardsGlobal>() {
            return global.0.clone();
        }
        let entity = cx.new(|_| Self::default());
        cx.set_global(PortForwardsGlobal(entity.clone()));
        entity
    }

    /// Starts forwarding `request` through `client`, or returns the address of
    /// the forward already running for it. Listens on the remote port's own
    /// number locally when it's free, else on any free port.
    pub fn start(
        &mut self,
        request: PortForwardRequest,
        client: kube::Client,
        cx: &mut Context<Self>,
    ) -> Result<SocketAddr, String> {
        if let Some(held) = self.held.get(&request) {
            return Ok(held.handle.forward().local_addr());
        }
        let listener = std::net::TcpListener::bind(("127.0.0.1", request.remote_port))
            .or_else(|_| std::net::TcpListener::bind(("127.0.0.1", 0)))
            .map_err(|error| format!("Couldn't listen locally: {error}"))?;
        let config = K8sPortForwardConfig {
            client,
            namespace: request.namespace.clone(),
            pod_name: request.pod.clone(),
            remote_port: request.remote_port,
        };
        let options = SupervisorOptions {
            health_check_interval: crate::consts::PORT_FORWARD_HEALTH_CHECK_INTERVAL,
            backoff: BackoffPolicy {
                initial: crate::consts::PORT_FORWARD_BACKOFF_INITIAL,
                max: crate::consts::PORT_FORWARD_BACKOFF_MAX,
            },
        };
        let forward = K8sPortForward::start(&crate::runtime::handle(cx), listener, config, options)
            .map_err(|error| format!("Couldn't start the forward: {error}"))?;
        let handle = self.registry.acquire(request.clone(), move || forward);
        let local_addr = handle.forward().local_addr();
        let follow = self.follow_state(&request, &handle, cx);
        self.held.insert(
            request,
            Held {
                handle,
                state: ForwardState::Connecting,
                _follow: follow,
            },
        );
        cx.notify();
        Ok(local_addr)
    }

    /// Relays the forward's state into the list as it changes.
    fn follow_state(
        &self,
        request: &PortForwardRequest,
        handle: &RegistryHandle<PortForwardRequest, K8sPortForward>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let mut states = handle.forward().state();
        let rx = crate::runtime::spawn_stream(cx, 4, move |tx| async move {
            loop {
                let state = *states.borrow_and_update();
                if tx.send(state).await.is_err() || states.changed().await.is_err() {
                    return;
                }
            }
        });
        let request = request.clone();
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, |state| {
                let _ = this.update(cx, |this, cx| {
                    if let Some(held) = this.held.get_mut(&request) {
                        held.state = state;
                        cx.notify();
                    }
                });
            })
            .await;
        })
    }

    /// Stops `request`'s forward. Whether there was one.
    pub fn stop(&mut self, request: &PortForwardRequest, cx: &mut Context<Self>) -> bool {
        let stopped = self.held.remove(request).is_some();
        if stopped {
            cx.notify();
        }
        stopped
    }

    /// Every running forward: what it forwards, where it listens, and its state.
    pub fn list(&self) -> Vec<(PortForwardRequest, SocketAddr, ForwardState)> {
        self.held
            .iter()
            .map(|(request, held)| {
                (
                    request.clone(),
                    held.handle.forward().local_addr(),
                    held.state,
                )
            })
            .collect()
    }

    /// The registry's live forwards - what is actually still running, as opposed
    /// to listed - for tests checking a stop released one.
    #[cfg(test)]
    pub fn live_requests(&self) -> std::collections::BTreeSet<PortForwardRequest> {
        self.registry.live_keys().borrow().clone()
    }
}

#[cfg(test)]
mod tests;
