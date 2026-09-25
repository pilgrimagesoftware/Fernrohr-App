//! Section 3.1 of the tunnel-subsystem change: `SshTunnel`, the `ManagedForward`
//! implementation that shells out to the system `ssh` client to hold open
//! `ssh -N -L <local>:<remote_host>:<remote_port> [-J ...] user@bastion`.
//!
//! This module owns spawning and supervising the child; `ForwardSupervisor` (section
//! 2.3) owns the retry state machine via the [`ForwardTransport`] impl below.
//! Readiness-by-probe and distinct auth/bind failure reasons are section 3.2; jump-host
//! chain coverage is section 3.3; process-group cleanup is section 3.4.

use crate::ssh_path::require_ssh_on_path;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
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

    /// Builds the argument list documented in task 3.1: `-N -L ...`, an optional `-J`
    /// chain, `ExitOnForwardFailure`/`ServerAliveInterval` always, `BatchMode` only when
    /// a key is configured, and the test-only known-hosts override when set.
    fn args(&self) -> Vec<String> {
        let mut args = vec![
            "-N".to_string(),
            "-L".to_string(),
            self.forward_spec(),
            "-p".to_string(),
            self.bastion_port.to_string(),
            "-o".to_string(),
            "ExitOnForwardFailure=yes".to_string(),
            "-o".to_string(),
            "ServerAliveInterval=15".to_string(),
        ];

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
}

impl SshTransport {
    #[allow(dead_code)]
    pub fn new(config: SshTunnelConfig) -> Self {
        Self {
            config,
            child: None,
        }
    }
}

impl crate::forward_supervisor::ForwardTransport for SshTransport {
    async fn connect(&mut self) -> Result<(), String> {
        let ssh_path = require_ssh_on_path()?;

        let child = Command::new(ssh_path)
            .args(self.config.args())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|err| format!("failed to spawn ssh: {err}"))?;
        self.child = Some(child);

        // Give ExitOnForwardFailure a moment to fail fast (auth rejected, remote bind
        // refused) before declaring the forward up; a real readiness probe against the
        // local port lands in section 3.2.
        tokio::time::sleep(Duration::from_millis(300)).await;

        match self.child.as_mut().expect("just spawned").try_wait() {
            Ok(None) => Ok(()),
            Ok(Some(status)) => {
                self.child = None;
                Err(format!("ssh exited before the forward came up: {status}"))
            }
            Err(err) => {
                self.child = None;
                Err(format!("failed to check ssh child status: {err}"))
            }
        }
    }

    async fn health_check(&mut self) -> Result<(), String> {
        let Some(child) = self.child.as_mut() else {
            return Err("ssh child was never spawned".to_string());
        };

        match child.try_wait() {
            Ok(None) => Ok(()),
            Ok(Some(status)) => {
                self.child = None;
                Err(format!("ssh exited: {status}"))
            }
            Err(err) => Err(format!("failed to check ssh child status: {err}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_include_forward_spec_and_reliability_options() {
        let config = SshTunnelConfig {
            bastion_user: "alice".to_string(),
            bastion_host: "bastion.example.com".to_string(),
            bastion_port: 22,
            jump_hosts: vec![],
            remote_host: "10.0.0.5".to_string(),
            remote_port: 6443,
            local_port: 40000,
            identity_file: None,
            known_hosts_file: None,
        };

        let args = config.args();
        assert_eq!(args[0], "-N");
        assert_eq!(args[1], "-L");
        assert_eq!(args[2], "40000:10.0.0.5:6443");
        assert!(args.contains(&"ExitOnForwardFailure=yes".to_string()));
        assert!(args.contains(&"ServerAliveInterval=15".to_string()));
        assert!(!args.contains(&"BatchMode=yes".to_string()));
        assert_eq!(args.last().unwrap(), "alice@bastion.example.com");
    }

    #[test]
    fn identity_file_enables_batch_mode() {
        let config = SshTunnelConfig {
            bastion_user: "alice".to_string(),
            bastion_host: "bastion.example.com".to_string(),
            bastion_port: 22,
            jump_hosts: vec![],
            remote_host: "10.0.0.5".to_string(),
            remote_port: 6443,
            local_port: 40000,
            identity_file: Some(PathBuf::from("/keys/id_ed25519")),
            known_hosts_file: None,
        };

        let args = config.args();
        assert!(args.contains(&"BatchMode=yes".to_string()));
        assert!(args.contains(&"-i".to_string()));
        assert!(args.contains(&"/keys/id_ed25519".to_string()));
    }

    #[test]
    fn jump_hosts_produce_a_dash_j_chain() {
        let config = SshTunnelConfig {
            bastion_user: "alice".to_string(),
            bastion_host: "bastion.example.com".to_string(),
            bastion_port: 22,
            jump_hosts: vec!["bob@hop1".to_string(), "carol@hop2".to_string()],
            remote_host: "10.0.0.5".to_string(),
            remote_port: 6443,
            local_port: 40000,
            identity_file: None,
            known_hosts_file: None,
        };

        let args = config.args();
        let jump_index = args.iter().position(|a| a == "-J").unwrap();
        assert_eq!(args[jump_index + 1], "bob@hop1,carol@hop2");
    }
}

/// Exercises `SshTransport` against a real local `sshd`, not a mock - the closest thing
/// to "a local sshd container" this machine can run without Docker. Proves the forward
/// actually comes `Up` and proxies bytes, not just that the child process spawns.
#[cfg(all(test, unix))]
mod sshd_integration {
    use super::*;
    use crate::forward_supervisor::ForwardTransport;
    use std::net::TcpListener;
    use std::process::Command as StdCommand;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A throwaway sshd + keypair rooted in a scratch directory, torn down on drop.
    struct LocalSshd {
        dir: PathBuf,
        sshd: std::process::Child,
        port: u16,
        client_key: PathBuf,
        known_hosts: PathBuf,
    }

    impl LocalSshd {
        fn spawn() -> Option<Self> {
            let sshd_path = which("sshd")?;
            let ssh_keygen = which("ssh-keygen")?;

            let dir =
                std::env::temp_dir().join(format!("fernrohr-sshd-test-{}", std::process::id()));
            std::fs::create_dir_all(&dir).ok()?;

            let host_key = dir.join("host_key");
            let client_key = dir.join("client_key");
            for key in [&host_key, &client_key] {
                let status = StdCommand::new(&ssh_keygen)
                    .args(["-q", "-t", "ed25519", "-N", ""])
                    .arg("-f")
                    .arg(key)
                    .status()
                    .ok()?;
                if !status.success() {
                    return None;
                }
            }

            let authorized_keys = dir.join("authorized_keys");
            std::fs::copy(client_key.with_extension("pub"), &authorized_keys).ok()?;

            // Trust the throwaway host key so the test doesn't depend on the real
            // ~/.ssh/known_hosts having (or not having) an entry for 127.0.0.1.
            let known_hosts = dir.join("known_hosts");
            let host_pub = std::fs::read_to_string(host_key.with_extension("pub")).ok()?;
            let host_pub_fields: Vec<&str> = host_pub.split_whitespace().collect();
            let known_hosts_line = format!(
                "[127.0.0.1]:{{PORT}} {} {}\n",
                host_pub_fields.first()?,
                host_pub_fields.get(1)?
            );

            let port = TcpListener::bind("127.0.0.1:0")
                .ok()?
                .local_addr()
                .ok()?
                .port();
            std::fs::write(
                &known_hosts,
                known_hosts_line.replace("{PORT}", &port.to_string()),
            )
            .ok()?;

            let user = std::env::var("USER").ok()?;
            let sshd_config = dir.join("sshd_config");
            std::fs::write(
                &sshd_config,
                format!(
                    "Port {port}\n\
                     ListenAddress 127.0.0.1\n\
                     HostKey {}\n\
                     AuthorizedKeysFile {}\n\
                     PidFile {}\n\
                     AllowUsers {user}\n\
                     PasswordAuthentication no\n\
                     KbdInteractiveAuthentication no\n\
                     PubkeyAuthentication yes\n\
                     UsePAM no\n\
                     StrictModes no\n\
                     PermitTTY no\n\
                     AllowTcpForwarding yes\n\
                     LogLevel ERROR\n",
                    host_key.display(),
                    authorized_keys.display(),
                    dir.join("sshd.pid").display(),
                )
                .as_bytes(),
            )
            .ok()?;

            let sshd = StdCommand::new(sshd_path)
                .arg("-f")
                .arg(&sshd_config)
                .arg("-D")
                .arg("-e")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok()?;

            // sshd needs a moment to bind before ssh dials it.
            std::thread::sleep(Duration::from_millis(400));

            Some(Self {
                dir,
                sshd,
                port,
                client_key,
                known_hosts,
            })
        }

        fn config(&self, remote_port: u16, local_port: u16) -> SshTunnelConfig {
            SshTunnelConfig {
                bastion_user: std::env::var("USER").unwrap(),
                bastion_host: "127.0.0.1".to_string(),
                bastion_port: self.port,
                jump_hosts: vec![],
                remote_host: "127.0.0.1".to_string(),
                remote_port,
                local_port,
                identity_file: Some(self.client_key.clone()),
                known_hosts_file: Some(self.known_hosts.clone()),
            }
        }
    }

    impl Drop for LocalSshd {
        fn drop(&mut self) {
            let _ = self.sshd.kill();
            let _ = self.sshd.wait();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn which(bin: &str) -> Option<PathBuf> {
        for dir in ["/usr/sbin", "/usr/bin", "/sbin", "/bin"] {
            let candidate = PathBuf::from(dir).join(bin);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }

    /// Binds a local echo listener and returns its port; each accepted connection
    /// echoes back whatever it reads until the peer closes.
    async fn spawn_echo_server() -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match stream.read(&mut buf).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => {
                                if stream.write_all(&buf[..n]).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }
                });
            }
        });
        port
    }

    #[tokio::test]
    async fn ssh_tunnel_forwards_a_tcp_echo_through_local_sshd() {
        let Some(sshd) = LocalSshd::spawn() else {
            eprintln!("skipping: no local sshd/ssh-keygen available to spawn a test bastion");
            return;
        };

        let echo_port = spawn_echo_server().await;
        let local_port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();

        let mut transport = SshTransport::new(sshd.config(echo_port, local_port));
        transport
            .connect()
            .await
            .expect("ssh forward should come up against the local test bastion");

        // The forward is asynchronous even once ssh has accepted the connection to the
        // bastion; retry the local dial briefly rather than racing it.
        let mut stream = None;
        for _ in 0..20 {
            match tokio::net::TcpStream::connect(("127.0.0.1", local_port)).await {
                Ok(s) => {
                    stream = Some(s);
                    break;
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
        let mut stream = stream.expect("forwarded local port should accept connections");

        stream.write_all(b"hello through the tunnel").await.unwrap();
        let mut buf = [0u8; 64];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello through the tunnel");

        transport
            .health_check()
            .await
            .expect("ssh child should still be running");
    }
}
