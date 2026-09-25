//! Crate-wide constants: values that are a *decision* (how often to poll, how long to
//! wait) rather than a single element's layout. See `.claude/rules/rust-structure.md`.

use std::time::Duration;

// UNWIRED(#3): `SshTransport` (tunnel-subsystem section 3) has no caller until section
// 6.2's connect-path integration, so dead_code analysis can't see these are reachable.
#[allow(dead_code)]
/// `SshTransport::connect` readiness probe: how long to keep retrying a local TCP
/// dial to the forwarded port before giving up and reporting a timeout.
pub(crate) const SSH_READINESS_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

#[allow(dead_code)]
/// `SshTransport::connect` readiness probe: delay between failed dial attempts.
pub(crate) const SSH_READINESS_POLL_INTERVAL: Duration = Duration::from_millis(50);
