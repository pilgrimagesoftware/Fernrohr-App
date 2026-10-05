//! A Pod port-forward as a [`ManagedForward`] (`k9s-remaining-keybindings` 4):
//! a local listener whose accept loop ([`super::port_forward::serve`]) bridges
//! each connection to the Pod, beside a [`ForwardSupervisor`] that reports the
//! forward's state from the Pod's own (Running or not). Dropping it closes the
//! listener and stops both tasks.

use super::port_forward::{K8sPortForwardConfig, PodPortForwardTransport, serve};
use crate::forward::managed::{ForwardHandle, ForwardState, ManagedForward};
use crate::forward::supervisor::{ForwardSupervisor, SupervisorOptions};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::runtime::Handle;
use tokio::sync::watch;

pub struct K8sPortForward {
    supervisor: ForwardSupervisor,
    /// The accept loop, aborted - closing the listener - when the forward goes.
    accept: tokio::task::JoinHandle<()>,
    acquired: Arc<AtomicUsize>,
}

impl K8sPortForward {
    /// Starts forwarding `listener`'s connections to `config`'s Pod port.
    pub fn start(
        rt: &Handle,
        listener: std::net::TcpListener,
        config: K8sPortForwardConfig,
        options: SupervisorOptions,
    ) -> std::io::Result<Self> {
        let local_addr = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let listener = {
            let _guard = rt.enter();
            tokio::net::TcpListener::from_std(listener)?
        };
        let supervisor = ForwardSupervisor::spawn(
            rt,
            local_addr,
            PodPortForwardTransport::new(config.clone()),
            options,
        );
        let accept = rt.spawn(serve(listener, config));
        Ok(Self {
            supervisor,
            accept,
            acquired: Arc::default(),
        })
    }
}

impl Drop for K8sPortForward {
    fn drop(&mut self) {
        self.accept.abort();
    }
}

impl ManagedForward for K8sPortForward {
    fn state(&self) -> watch::Receiver<ForwardState> {
        self.supervisor.state()
    }

    fn local_addr(&self) -> SocketAddr {
        self.supervisor.local_addr()
    }

    fn acquire(&self) -> ForwardHandle {
        self.acquired.fetch_add(1, Ordering::SeqCst);
        let acquired = self.acquired.clone();
        ForwardHandle::new(move || {
            acquired.fetch_sub(1, Ordering::SeqCst);
        })
    }
}
