//! Section 4 of the tunnel-subsystem change: the `kube`-backed counterpart to
//! `SshTunnel` (section 3). Unlike `ssh -L`, which spawns a child that owns its own
//! local listening socket, a `kube` port-forward is a per-connection WebSocket stream -
//! there is no single long-lived "the forward" to hold open the way `ssh` holds one.
//! So this module owns the local `TcpListener`'s accept loop itself: each accepted
//! connection opens a *fresh* `Api::<Pod>::portforward` call and is bridged to it
//! directly with `copy_bidirectional`, torn down independently when either side closes.
//!
//! [`PodPortForwardTransport`] plugs into `ForwardSupervisor` (section 2.3) the same
//! way `SshTransport` does, but "connect"/"health_check" here don't hold a data path
//! open - they confirm the target Pod exists and is `Running`, which is what "ready to
//! accept forwarded connections" means for a Pod-backed forward. Target loss (Pod
//! deleted mid-session) is section 4.2.

use crate::consts::PORT_FORWARD_ACCEPT_ERROR_PAUSE;
use crate::forward::supervisor::ForwardTransport;
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;

/// Everything needed to hold one Pod port-forward open: which Pod/port to forward to.
#[derive(Clone)]
pub struct K8sPortForwardConfig {
    pub client: Client,
    pub namespace: String,
    pub pod_name: String,
    pub remote_port: u16,
}

impl K8sPortForwardConfig {
    fn pods(&self) -> Api<Pod> {
        Api::namespaced(self.client.clone(), &self.namespace)
    }
}

/// The [`ForwardTransport`] `K8sPortForward` plugs into `ForwardSupervisor`: readiness
/// and health both mean "the Pod exists and is `Running`". There is no persistent
/// process to supervise the way `SshTransport` supervises `ssh` - the actual data path
/// is a fresh portforward per local connection, handled by [`serve`] below.
pub struct PodPortForwardTransport {
    config: K8sPortForwardConfig,
}

impl PodPortForwardTransport {
    pub fn new(config: K8sPortForwardConfig) -> Self {
        Self { config }
    }
}

impl ForwardTransport for PodPortForwardTransport {
    async fn connect(&mut self) -> Result<(), String> {
        check_pod_running(&self.config.pods(), &self.config.pod_name).await
    }

    async fn health_check(&mut self) -> Result<(), String> {
        check_pod_running(&self.config.pods(), &self.config.pod_name).await
    }
}

async fn check_pod_running(pods: &Api<Pod>, pod_name: &str) -> Result<(), String> {
    let pod = pods.get(pod_name).await.map_err(|err| {
        format!(
            "failed to get pod {pod_name}: {}",
            crate::k8s::error::describe(&err)
        )
    })?;
    match pod.status.as_ref().and_then(|s| s.phase.as_deref()) {
        Some("Running") => Ok(()),
        Some(other) => Err(format!("pod {pod_name} is {other}, not Running")),
        None => Err(format!("pod {pod_name} has no status phase")),
    }
}

/// Bridges one accepted local connection to a fresh port-forward stream on
/// `config.pod_name`. `kube`'s `Portforwarder` multiplexes by port, not by client
/// connection, so there is no connection sharing to do here the way `ssh -L` shares one
/// child process across every local connection - each local connection is its own
/// independent WebSocket upgrade to the API server.
async fn bridge_connection(
    config: &K8sPortForwardConfig,
    mut local: TcpStream,
) -> Result<(), String> {
    let remote_port = config.remote_port;
    let mut forwarder = config
        .pods()
        .portforward(&config.pod_name, &[remote_port])
        .await
        .map_err(|err| {
            format!(
                "failed to open port-forward to {}:{remote_port}: {}",
                config.pod_name,
                crate::k8s::error::describe(&err)
            )
        })?;
    let mut remote = forwarder
        .take_stream(remote_port)
        .ok_or_else(|| format!("no stream returned for port {remote_port}"))?;

    tokio::io::copy_bidirectional(&mut local, &mut remote)
        .await
        .map(|_| ())
        .map_err(|err| format!("port-forward copy failed: {err}"))
}

/// Runs the accept loop for one local port-forward listener: every accepted connection
/// is bridged (section 4.1) on its own task so one slow or failed forward can't block
/// the next connection. Runs until it is dropped: [`super::managed::K8sPortForward`]
/// owns the task this runs on and aborts it when the forward is dropped, which also
/// ends every connection it bridged.
pub async fn serve(listener: TcpListener, config: K8sPortForwardConfig) {
    serve_connections(
        || async { listener.accept().await.map(|(local, _peer)| local) },
        move |local| {
            let config = config.clone();
            async move { bridge_connection(&config, local).await }
        },
    )
    .await;
}

/// [`serve`]'s loop, apart from the socket and the cluster so tests can drive it.
///
/// The bridged connections live in a `JoinSet` this future owns, so dropping or
/// aborting it aborts them too: a stopped forward leaves nothing running. A failed
/// accept pauses for [`PORT_FORWARD_ACCEPT_ERROR_PAUSE`] before the next try.
async fn serve_connections<A, AF, B, BF>(mut accept: A, bridge: B)
where
    A: FnMut() -> AF,
    AF: Future<Output = std::io::Result<TcpStream>>,
    B: Fn(TcpStream) -> BF,
    BF: Future<Output = Result<(), String>> + Send + 'static,
{
    let mut connections = JoinSet::new();
    loop {
        match accept().await {
            Ok(local) => {
                let connection = bridge(local);
                connections.spawn(async move {
                    if let Err(err) = connection.await {
                        log::warn!("k8s port-forward connection failed: {err}");
                    }
                });
            }
            Err(err) => {
                log::warn!("k8s port-forward listener accept failed: {err}");
                tokio::time::sleep(PORT_FORWARD_ACCEPT_ERROR_PAUSE).await;
            }
        }
        reap(&mut connections);
    }
}

/// Takes finished connections out of `connections`, so it holds only live ones,
/// logging any that panicked. Returns how many did.
fn reap(connections: &mut JoinSet<()>) -> usize {
    let mut panicked = 0;
    while let Some(finished) = connections.try_join_next() {
        if let Err(error) = finished
            && error.is_panic()
        {
            log::error!("k8s port-forward connection task panicked: {error}");
            panicked += 1;
        }
    }
    panicked
}

#[cfg(test)]
mod tests;
