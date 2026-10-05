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
