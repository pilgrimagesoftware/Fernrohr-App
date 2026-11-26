//! Section 6.2 of the tunnel-subsystem change: `SshTunnel`, the [`ManagedForward`]
//! wrapper the connect path acquires from a [`crate::forward::registry::ForwardRegistry`].
//!
//! `SshTransport`/`SshTunnelConfig` (this module's parent) and `ForwardSupervisor`
//! (section 2.3) already implement spawn/supervise and the retry state machine; this
//! type is just the glue that makes an `ssh` forward satisfy `ManagedForward` so the
//! registry can share and refcount it. `acquire()` is a no-op handle rather than real
//! start/stop bookkeeping: the registry already ties the forward's lifetime to the
//! `Arc<SshTunnel>`'s last `Weak` upgrade (see `forward_registry.rs`), and dropping the
//! last one drops this struct, which aborts the supervisor task via its own `Drop`.

use super::{SshTransport, SshTunnelConfig};
use crate::forward::managed::{ForwardHandle, ForwardState, ManagedForward};
use crate::forward::supervisor::{ForwardSupervisor, SupervisorOptions};
use std::io::{self, Write as _};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
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
    /// The file is created already private, in one step (`O_CREAT|O_EXCL`, mode
    /// 0600): there is no moment when the key is readable by others, and a file
    /// or symlink already at the path is refused rather than written through.
    /// Each call gets its own name - a tunnel's key is written on every acquire,
    /// and two acquires may overlap.
    pub(crate) fn write(tunnel_id: &str, secret: &str) -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fernrohr-tunnel-{tunnel_id}-{}-{}.pem",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = private_file(&path)?;
        // Owned from here: a failed write still removes the file.
        let written = Self { path };
        file.write_all(secret.as_bytes())?;
        Ok(written)
    }

    pub(crate) fn path(&self) -> PathBuf {
        self.path.clone()
    }
}

#[cfg(unix)]
fn private_file(path: &std::path::Path) -> io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn private_file(path: &std::path::Path) -> io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
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
    pid: crate::util::pidfile::PidSlot,
}

impl SshTunnel {
    /// Spawns the supervised forward. `identity_file`, if the tunnel has a stored
    /// secret, must already be written (via [`TransientIdentityFile::write`]) with its
    /// path set on `config.identity_file` - building it is fallible (a temp-dir write),
    /// so callers do it before this infallible spawn rather than inside a
    /// [`crate::forward::registry::ForwardRegistry`] factory closure, which can't fail.
    pub(crate) fn spawn(
        rt: &Handle,
        config: SshTunnelConfig,
        identity_file: Option<TransientIdentityFile>,
        local_addr: SocketAddr,
        options: SupervisorOptions,
    ) -> Self {
        let pid = crate::util::pidfile::PidSlot::default();
        let transport = SshTransport::new(config).with_pid_slot(pid.clone());
        let supervisor = ForwardSupervisor::spawn(rt, local_addr, transport, options);
        Self {
            supervisor,
            _identity_file: identity_file,
            pid,
        }
    }

    /// The running `ssh`'s pid, while there is one.
    pub fn pid(&self) -> Option<u32> {
        self.pid.get()
    }
}

impl SshTunnel {
    /// Where the supervisor leaves its reason if it gives up.
    pub fn failure(&self) -> crate::forward::supervisor::FailureSlot {
        self.supervisor.failure()
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

#[cfg(test)]
mod tests {
    use super::TransientIdentityFile;

    /// Security pass: the key file is private from the moment it exists, holds
    /// the key, gets its own name per write, and is gone once dropped.
    #[test]
    fn an_identity_file_is_private_unique_and_removed_on_drop() {
        let first = TransientIdentityFile::write("t-test", "KEY-ONE").unwrap();
        let second = TransientIdentityFile::write("t-test", "KEY-TWO").unwrap();
        assert_ne!(first.path(), second.path());
        assert_eq!(std::fs::read_to_string(first.path()).unwrap(), "KEY-ONE");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(first.path())
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let path = first.path();
        drop(first);
        assert!(!path.exists());
        assert!(second.path().exists());
    }
}
