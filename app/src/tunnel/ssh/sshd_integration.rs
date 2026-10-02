//! Exercises `SshTransport` against a real local `sshd`, not a mock - the closest thing
//! to "a local sshd container" this machine can run without Docker. Proves the forward
//! actually comes `Up` and proxies bytes, not just that the child process spawns.

mod fixture;

mod cleanup;
mod forwarding;
mod jump_hosts;
mod test_connection;
