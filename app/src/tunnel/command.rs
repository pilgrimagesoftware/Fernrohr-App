//! `command-tunnels`: a tunnel whose forward is a command the user supplies - a
//! vendor CLI opening an SSH session through an identity-aware proxy, say - rather
//! than Fernrohr's own `ssh -N -L`. [`argv`] turns the stored command line into the
//! arguments that run; [`transport`] runs and supervises them, with [`process`]
//! spawning and stopping the process group and [`output`] keeping its recent output.
//! [`CommandTunnel`] is the supervised forward the tunnel registry shares.

pub(crate) mod argv;
mod handle;
mod output;
mod process;
mod transport;

pub use handle::CommandTunnel;
pub(crate) use transport::CommandTransport;

use crate::config::tunnels::CommandTunnelConfig;
use crate::forward::supervisor::ForwardTransport as _;

/// The Tunnels window's Test for a command tunnel: runs its command until the local
/// port answers - or it exits, or the startup timeout passes - then stops it. Never
/// goes through the tunnel registry, and leaves nothing running either way.
pub async fn test_command(config: &CommandTunnelConfig) -> Result<(), String> {
    let args = argv::split(&config.command_line)
        .map_err(|error| format!("the command can't be run: {error:?}"))?;
    let port = match config.local_port {
        Some(port) => port,
        None => crate::util::port_allocator::allocate()
            .map_err(|error| format!("couldn't allocate a local port: {error}"))?
            .addr()
            .port(),
    };
    let args = argv::substitute_port(&args, port);
    let timeout = std::time::Duration::from_secs(config.startup_timeout_secs);
    let mut transport = CommandTransport::new(args, port, timeout);
    let result = transport.connect().await;
    transport.stop().await;
    result
}
