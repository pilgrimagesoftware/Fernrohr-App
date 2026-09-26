//! Section 3.1 of the tunnel-subsystem change: `SshTunnel`, the `ManagedForward`
//! implementation that shells out to the system `ssh` client to hold open
//! `ssh -N -L <local>:<remote_host>:<remote_port> [-J ...] user@bastion`.
//!
//! This module owns spawning and supervising the child; `ForwardSupervisor` (section
//! 2.3) owns the retry state machine via the [`ForwardTransport`] impl below.
//! Readiness-by-probe and distinct auth/bind failure reasons are section 3.2; jump-host
//! chain coverage is section 3.3. Section 3.4 (this module plus [`crate::pidfile`])
//! covers process-group cleanup: `ssh -J` resolves each jump hop through a *separate*
//! `ssh` subprocess (see `SshTunnelConfig::ssh_config_file`'s doc comment), so
//! `kill_on_drop` alone - which only signals the direct child pid - leaves those hop
//! processes running. `ssh` is spawned into its own process group (`process_group(0)`,
//! unix-only) so the whole tree can be torn down with one `killpg`-equivalent signal.

use crate::consts::{SSH_READINESS_POLL_INTERVAL, SSH_READINESS_PROBE_TIMEOUT};
use crate::pidfile;
use crate::ssh_path::require_ssh_on_path;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

/// Everything needed to hold open one `ssh -L` forward. `identity_file` and
/// `known_hosts_file` are paths, not secrets themselves - the actual key material stays
/// on disk (or, once section 5.2 lands, is written to a transient file from the
/// keychain) rather than passing through this struct.
// UNWIRED(#3): section 5's tunnel config UI and section 6.2's connect-path integration
// are the first real callers; today only this module's own tests build one.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct SshTunnelConfig {
    pub bastion_user: String,
    pub bastion_host: String,
    pub bastion_port: u16,
    /// Extra `user@host[:port]` hops for `-J`, nearest-to-target last. Section 3.3.
    pub jump_hosts: Vec<String>,
    pub remote_host: String,
    pub remote_port: u16,
    pub local_port: u16,
    /// Present when a secret (private key) is configured for this tunnel - selects
    /// `BatchMode=yes` so a misconfigured or passphrase-protected key fails fast
    /// instead of `ssh` blocking on an interactive prompt no one can answer.
    pub identity_file: Option<PathBuf>,
    /// Overrides `UserKnownHostsFile`/`StrictHostKeyChecking` for tests against a
    /// throwaway local sshd; production tunnels leave this `None` and get ssh's normal
    /// known-hosts behavior.
    pub known_hosts_file: Option<PathBuf>,
    /// Test-only `-F <path>` config file override for the spawned `ssh`. `-J` hops are
    /// not driven by this struct's own `-o` flags: OpenSSH resolves each `-J` hop by
    /// launching a *separate* `ssh` subprocess (`ProxyCommand=ssh ... -W %h:%p <hop>`)
    /// that, by default, re-resolves `~/.ssh/config` from the real user's home
    /// directory (via the password database, not the `$HOME` env var, so overriding
    /// `$HOME` on the child process has no effect) - confirmed by tracing a real `-J`
    /// connection with `-vvv`. `-F`, unlike `$HOME`, is passed through verbatim to that
    /// nested subprocess, so it is the only override that reaches a `-J` hop. Production
    /// tunnels rely on the user's real `~/.ssh/config` already trusting the jump hosts,
    /// so this stays `None`; the section 3.3 chain test points it at a scratch config
    /// aliasing the jump hop to the throwaway key and known_hosts file.
    pub ssh_config_file: Option<PathBuf>,
}

// UNWIRED(#3): only exercised by this module's own tests until SshTransport::connect
// (below) and section 6.2's connect-path integration call `args()` for real.
#[allow(dead_code)]
impl SshTunnelConfig {
    fn jump_flag(&self) -> Option<String> {
        (!self.jump_hosts.is_empty()).then(|| self.jump_hosts.join(","))
    }

    fn bastion_target(&self) -> String {
        format!("{}@{}", self.bastion_user, self.bastion_host)
    }

    fn forward_spec(&self) -> String {
        format!(
            "{}:{}:{}",
            self.local_port, self.remote_host, self.remote_port
        )
    }

    /// Builds the argument list documented in task 3.1: an optional test-only `-F`
    /// config override first, then `-N -L ...`, an optional `-J` chain,
    /// `ExitOnForwardFailure`/`ServerAliveInterval` always, `BatchMode` only when a key
    /// is configured, and the test-only known-hosts override when set.
    fn args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(config_file) = &self.ssh_config_file {
            args.push("-F".to_string());
            args.push(config_file.display().to_string());
        }
        args.extend([
            "-N".to_string(),
            "-L".to_string(),
            self.forward_spec(),
            "-p".to_string(),
            self.bastion_port.to_string(),
            "-o".to_string(),
            "ExitOnForwardFailure=yes".to_string(),
            "-o".to_string(),
            "ServerAliveInterval=15".to_string(),
        ]);

        if let Some(jump) = self.jump_flag() {
            args.push("-J".to_string());
            args.push(jump);
        }

        if let Some(identity_file) = &self.identity_file {
            args.push("-o".to_string());
            args.push("BatchMode=yes".to_string());
            args.push("-i".to_string());
            args.push(identity_file.display().to_string());
        }

        if let Some(known_hosts) = &self.known_hosts_file {
            args.push("-o".to_string());
            args.push(format!("UserKnownHostsFile={}", known_hosts.display()));
            args.push("-o".to_string());
            args.push("StrictHostKeyChecking=yes".to_string());
        }

        args.push(self.bastion_target());
        args
    }
}

/// The [`crate::forward_supervisor::ForwardTransport`] `SshTunnel` plugs into
/// `ForwardSupervisor`: `connect` spawns the child, `health_check` confirms it's still
/// running. A distinct exit reason per failure mode is section 3.2.
// UNWIRED(#3): section 6.2's connect-path integration is the first real caller that
// hands one of these to `ForwardSupervisor::spawn`.
#[allow(dead_code)]
pub struct SshTransport {
    config: SshTunnelConfig,
    child: Option<Child>,
    /// Set once `connect` spawns a child; recorded so `Drop` can `killpg` the whole
    /// process group (not just the direct child) and remove the pidfile that lets a
    /// future app startup sweep this forward if the process crashes first. Section 3.4.
    pidfile: Option<pidfile::PidFile>,
}

impl SshTransport {
    #[allow(dead_code)]
    pub fn new(config: SshTunnelConfig) -> Self {
        Self {
            config,
            child: None,
            pidfile: None,
        }
    }

    /// The spawned child's pid, once `connect` has run. Test-only: lets
    /// `sshd_integration`'s section 3.4 test confirm the whole process group (direct
    /// child plus any `-J` hop, which inherits the same group) is gone after `Drop`.
    #[cfg(test)]
    pub(crate) fn child_pid(&self) -> Option<u32> {
        self.child.as_ref().and_then(|child| child.id())
    }
}

impl Drop for SshTransport {
    /// `kill_on_drop(true)` on the `Command` below only signals the direct `ssh` pid;
    /// a `-J` jump-host hop runs as a separate `ssh` subprocess in the same process
    /// group (see `process_group(0)` in `connect`) that would otherwise survive. Kill
    /// the whole group here and drop the pidfile so a startup sweep never sees it.
    fn drop(&mut self) {
        if let Some(child) = self.child.as_ref()
            && let Some(pid) = child.id()
        {
            pidfile::kill_process_group(pid);
        }
    }
}

impl crate::forward_supervisor::ForwardTransport for SshTransport {
    /// Declares the forward ready only once the local port actually accepts a
    /// connection, polling at [`SSH_READINESS_POLL_INTERVAL`] up to
    /// [`SSH_READINESS_PROBE_TIMEOUT`]. If `ssh` exits before that (auth rejected,
    /// remote bind refused, ...) the failure reason is classified from its stderr so
    /// callers can tell an auth failure from a bind failure - see
    /// `classify_exit_failure`.
    async fn connect(&mut self) -> Result<(), String> {
        let ssh_path = require_ssh_on_path()?;

        let mut command = Command::new(ssh_path);
        command
            .args(self.config.args())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // Put ssh in its own process group (pgid = its own pid) so a -J jump-host hop,
        // which runs as a separate ssh subprocess inheriting this group, can be torn
        // down in one signal rather than surviving the direct child's death. Unix only;
        // there is no Windows job-object equivalent wired up here yet.
        #[cfg(unix)]
        command.process_group(0);

        let mut child = command
            .spawn()
            .map_err(|err| format!("failed to spawn ssh: {err}"))?;
        let mut stderr = child.stderr.take().expect("stderr was piped above");
        let local_port = self.config.local_port;

        if let Some(pid) = child.id() {
            self.pidfile = pidfile::PidFile::write(pid).ok();
        }

        let deadline = tokio::time::Instant::now() + SSH_READINESS_PROBE_TIMEOUT;
        let exit_status = loop {
            if tokio::net::TcpStream::connect(("127.0.0.1", local_port))
                .await
                .is_ok()
            {
                break None;
            }

            match child.try_wait() {
                Ok(None) => {}
                Ok(Some(status)) => break Some(status),
                Err(err) => {
                    let _ = child.start_kill();
                    self.pidfile = None;
                    return Err(format!("failed to check ssh child status: {err}"));
                }
            }

            if tokio::time::Instant::now() >= deadline {
                let _ = child.start_kill();
                let _ = child.wait().await;
                self.pidfile = None;
                return Err("timed out waiting for the local forward to become ready".to_string());
            }

            tokio::time::sleep(SSH_READINESS_POLL_INTERVAL).await;
        };

        let Some(status) = exit_status else {
            self.child = Some(child);
            return Ok(());
        };

        self.pidfile = None;
        let mut stderr_output = String::new();
        let _ = stderr.read_to_string(&mut stderr_output).await;
        Err(classify_exit_failure(status, &stderr_output))
    }

    async fn health_check(&mut self) -> Result<(), String> {
        let Some(child) = self.child.as_mut() else {
            return Err("ssh child was never spawned".to_string());
        };

        match child.try_wait() {
            Ok(None) => Ok(()),
            Ok(Some(status)) => {
                self.child = None;
                self.pidfile = None;
                Err(format!("ssh exited: {status}"))
            }
            Err(err) => Err(format!("failed to check ssh child status: {err}")),
        }
    }
}

/// `ssh`'s own wording for each failure mode is stable enough to match on: publickey/
/// password rejection always says "Permission denied", and both `ExitOnForwardFailure`
/// bind failures ("cannot listen to port", "Address already in use") and remote refusal
/// ("forwarding request failed") name forwarding explicitly.
// UNWIRED(#3): only `SshTransport::connect` calls this today, and that impl is itself
// unwired until section 6.2's connect-path integration; dead_code analysis can't see
// through the unused `SshTransport` to know this is reachable.
#[allow(dead_code)]
fn classify_exit_failure(status: ExitStatus, stderr_output: &str) -> String {
    let lower = stderr_output.to_lowercase();

    if lower.contains("permission denied") || lower.contains("authentication failed") {
        return format!("ssh authentication failed: {}", stderr_output.trim());
    }

    if lower.contains("cannot listen")
        || lower.contains("address already in use")
        || lower.contains("forwarding request failed")
        || lower.contains("bind:")
    {
        return format!("ssh forward failed to bind: {}", stderr_output.trim());
    }

    if stderr_output.trim().is_empty() {
        format!("ssh exited before the forward came up: {status}")
    } else {
        format!(
            "ssh exited before the forward came up: {status}: {}",
            stderr_output.trim()
        )
    }
}

mod handle;
pub use handle::SshTunnel;
pub(crate) use handle::TransientIdentityFile;

#[cfg(test)]
mod tests;

#[cfg(all(test, unix))]
mod sshd_integration;
