//! [`CommandTunnel`]: a [`CommandTransport`] under a [`ForwardSupervisor`], so a
//! command tunnel is a [`ManagedForward`] the tunnel registry can share and
//! refcount, exactly as `SshTunnel` is for an SSH tunnel. Dropping the last handle
//! drops this, which aborts the supervisor and so stops the command's process group.

use super::CommandTransport;
use crate::config::tunnels::CommandTunnelMode;
use crate::forward::managed::{ForwardHandle, ForwardState, ManagedForward};
use crate::forward::supervisor::{ForwardSupervisor, SupervisorOptions};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::sync::watch;

pub struct CommandTunnel {
    supervisor: ForwardSupervisor,
    mode: CommandTunnelMode,
}

impl CommandTunnel {
    /// Starts supervising `argv` (`{port}` already substituted) listening on
    /// `local_addr`.
    pub(crate) fn spawn(
        rt: &Handle,
        argv: Vec<String>,
        local_addr: SocketAddr,
        startup_timeout: Duration,
        mode: CommandTunnelMode,
        options: SupervisorOptions,
    ) -> Self {
        let transport = CommandTransport::new(argv, local_addr.port(), startup_timeout);
        Self {
            supervisor: ForwardSupervisor::spawn(rt, local_addr, transport, options),
            mode,
        }
    }

    /// What the local port offers: a proxy, or the API server itself.
    pub fn mode(&self) -> CommandTunnelMode {
        self.mode
    }
}

impl ManagedForward for CommandTunnel {
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
