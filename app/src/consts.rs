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

// UNWIRED(#3): `tunnel_store::TunnelStore` (section 5.3) is the first real caller of
// the section 5.2 keychain wrapper this backs.
#[allow(dead_code)]
/// `keyring` service name for tunnel secrets (`account` is the tunnel id). A distinct
/// service from `keychain.rs`'s smoke-test constant so a spike credential never
/// collides with a real tunnel's stored key.
pub(crate) const TUNNEL_KEYCHAIN_SERVICE: &str = "com.pilgrimagesoftware.fernrohr.tunnels";

/// `connection-status-bar` design.md decision 2: a paused connection's status bar item
/// escalates from its reason's usual color to danger once it has been paused this long.
/// A single named constant so the "arbitrary" 30 seconds (design.md's own word) is easy
/// to find and retune after real use, rather than a literal buried in `severity`.
pub(crate) const STATUS_ESCALATE_AFTER: Duration = Duration::from_secs(30);

/// `connection-status-bar` design decision 4: how often the status bar refreshes elapsed
/// time and re-checks the escalation threshold while any item it shows is not connected.
/// Idle (fully connected) windows run no timer at all, so this only ever wakes a window
/// with a problem to show.
pub(crate) const STATUS_TICK_INTERVAL: Duration = Duration::from_secs(1);
