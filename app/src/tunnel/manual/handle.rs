//! [`ManualTunnel`]: a [`ManualTransport`] under a [`ForwardSupervisor`], so a manual
//! tunnel is a [`ManagedForward`] the tunnel registry can share and refcount, as the
//! SSH and command tunnels are. It listens on nothing: connections through it keep
//! their own API server address (`TunnelRoute::Direct`). Dropping the last handle
//! drops this, which aborts the supervisor and so withdraws a pending confirmation.

use super::ManualTransport;
use crate::forward::managed::{ForwardHandle, ForwardState, ManagedForward};
use crate::forward::supervisor::{FailureSlot, ForwardSupervisor, SupervisorOptions};
use std::net::SocketAddr;
use tokio::runtime::Handle;
use tokio::sync::watch;

pub struct ManualTunnel {
    supervisor: ForwardSupervisor,
}

impl ManualTunnel {
    pub(crate) fn spawn(
        rt: &Handle,
        transport: ManualTransport,
        options: SupervisorOptions,
    ) -> Self {
        // No local port: nothing listens, and the route never reads the address.
        let unused = SocketAddr::from(([127, 0, 0, 1], 0));
        Self {
            supervisor: ForwardSupervisor::spawn(rt, unused, transport, options),
        }
    }

    /// Where the supervisor leaves its reason if the user cancels.
    pub fn failure(&self) -> FailureSlot {
        self.supervisor.failure()
    }
}

impl ManagedForward for ManualTunnel {
    fn state(&self) -> watch::Receiver<ForwardState> {
        self.supervisor.state()
    }

    fn local_addr(&self) -> SocketAddr {
        self.supervisor.local_addr()
    }

    fn acquire(&self) -> ForwardHandle {
        ForwardHandle::new(|| {})
    }
}
