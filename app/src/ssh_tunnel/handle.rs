//! Section 6.2 of the tunnel-subsystem change: `SshTunnel`, the [`ManagedForward`]
//! wrapper the connect path acquires from a [`crate::forward_registry::ForwardRegistry`].
//!
//! `SshTransport`/`SshTunnelConfig` (this module's parent) and `ForwardSupervisor`
//! (section 2.3) already implement spawn/supervise and the retry state machine; this
//! type is just the glue that makes an `ssh` forward satisfy `ManagedForward` so the
//! registry can share and refcount it. `acquire()` is a no-op handle rather than real
//! start/stop bookkeeping: the registry already ties the forward's lifetime to the
//! `Arc<SshTunnel>`'s last `Weak` upgrade (see `forward_registry.rs`), and dropping the
//! last one drops this struct, which aborts the supervisor task via its own `Drop`.

use super::{SshTransport, SshTunnelConfig};
use crate::forward_supervisor::{ForwardSupervisor, SupervisorOptions};
use crate::managed_forward::{ForwardHandle, ForwardState, ManagedForward};
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::runtime::Handle;
use tokio::sync::watch;

/// Writes `secret` to a private (0600 on unix) file under the system temp directory and
/// removes it on `Drop`, so an `ssh -i` identity file exists only as long as the
/// `SshTunnel` that needs it - never written to the app's own config/cache directories,
/// which aren't guaranteed private.
pub(crate) struct TransientIdentityFile {
    path: PathBuf,
}

impl TransientIdentityFile {
    pub(crate) fn write(tunnel_id: &str, secret: &str) -> io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "fernrohr-tunnel-{tunnel_id}-{}.pem",
            std::process::id()
        ));
        std::fs::write(&path, secret)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(Self { path })
    }

    pub(crate) fn path(&self) -> PathBuf {
        self.path.clone()
    }
}

impl Drop for TransientIdentityFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// A live `ssh -L` forward: an [`SshTransport`] supervised by a [`ForwardSupervisor`],
/// plus the transient identity file (if the tunnel has a stored secret) kept alive for
/// as long as `ssh` might need to re-read it across a reconnect.
pub struct SshTunnel {
    supervisor: ForwardSupervisor,
    _identity_file: Option<TransientIdentityFile>,
}

impl SshTunnel {
    /// Spawns the supervised forward. `identity_file`, if the tunnel has a stored
    /// secret, must already be written (via [`TransientIdentityFile::write`]) with its
    /// path set on `config.identity_file` - building it is fallible (a temp-dir write),
    /// so callers do it before this infallible spawn rather than inside a
    /// [`crate::forward_registry::ForwardRegistry`] factory closure, which can't fail.
    pub(crate) fn spawn(
        rt: &Handle,
        config: SshTunnelConfig,
        identity_file: Option<TransientIdentityFile>,
        local_addr: SocketAddr,
        options: SupervisorOptions,
    ) -> Self {
        let transport = SshTransport::new(config);
        let supervisor = ForwardSupervisor::spawn(rt, local_addr, transport, options);
        Self {
            supervisor,
            _identity_file: identity_file,
        }
    }
}

impl ManagedForward for SshTunnel {
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
