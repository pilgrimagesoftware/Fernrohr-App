//! `command-tunnels`: a tunnel whose forward is a command the user supplies - a
//! vendor CLI opening an SSH session through an identity-aware proxy, say - rather
//! than Fernrohr's own `ssh -N -L`. [`argv`] turns the stored command line into the
//! arguments that run; [`transport`] runs and supervises them, with [`process`]
//! spawning and stopping the process group and [`output`] keeping its recent output.
// UNWIRED(#126): the tunnel registry (section 3) is the first caller of the transport
// outside its own tests.
#![allow(dead_code, unused_imports)]

pub(crate) mod argv;
mod output;
mod process;
mod transport;

pub(crate) use transport::CommandTransport;
