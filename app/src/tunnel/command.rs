//! `command-tunnels`: a tunnel whose forward is a command the user supplies - a
//! vendor CLI opening an SSH session through an identity-aware proxy, say - rather
//! than Fernrohr's own `ssh -N -L`. [`argv`] turns the stored command line into the
//! arguments that run.

pub(crate) mod argv;
