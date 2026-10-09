//! [`TunnelForward`]: the one forward type the tunnel registry holds, whichever kind
//! of tunnel started it, and [`TunnelRoute`]: how a bound context's client goes
//! through it.

use crate::config::tunnels::CommandTunnelMode;
use crate::forward::managed::{ForwardHandle, ForwardState, ManagedForward};
use crate::forward::supervisor::FailureSlot;
use crate::tunnel::command::CommandTunnel;
use crate::tunnel::manual::ManualTunnel;
use crate::tunnel::ssh::SshTunnel;
use std::net::SocketAddr;
use tokio::sync::watch;

/// How a bound context's client reaches its API server through its tunnel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TunnelRoute {
    /// The local port is the API server: the client's address is rewritten to it,
    /// with the TLS server name pinned to the real host. SSH and forward-mode
    /// command tunnels.
    Rewrite,
    /// The local port is an HTTP proxy: the client keeps its real address and sends
    /// its traffic through the proxy. Proxy-mode command tunnels.
    Proxy,
    /// Nothing to go through: the client keeps its own address, and connects once
    /// the tunnel is confirmed. Manual tunnels.
    Direct,
}

pub enum TunnelForward {
    Ssh(SshTunnel),
    Command(CommandTunnel),
    Manual(ManualTunnel),
}

impl TunnelForward {
    pub fn route(&self) -> TunnelRoute {
        match self {
            Self::Ssh(_) => TunnelRoute::Rewrite,
            Self::Command(tunnel) => match tunnel.mode() {
                CommandTunnelMode::Proxy => TunnelRoute::Proxy,
                CommandTunnelMode::Forward => TunnelRoute::Rewrite,
            },
            Self::Manual(_) => TunnelRoute::Direct,
        }
    }

    /// Where the forward's supervisor leaves its reason if it gives up.
    pub fn failure(&self) -> FailureSlot {
        match self {
            Self::Ssh(tunnel) => tunnel.failure(),
            Self::Command(tunnel) => tunnel.failure(),
            Self::Manual(tunnel) => tunnel.failure(),
        }
    }

    fn inner(&self) -> &dyn ManagedForward {
        match self {
            Self::Ssh(tunnel) => tunnel,
            Self::Command(tunnel) => tunnel,
            Self::Manual(tunnel) => tunnel,
        }
    }
}

impl ManagedForward for TunnelForward {
    fn state(&self) -> watch::Receiver<ForwardState> {
        self.inner().state()
    }

    fn local_addr(&self) -> SocketAddr {
        self.inner().local_addr()
    }

    fn acquire(&self) -> ForwardHandle {
        self.inner().acquire()
    }
}
