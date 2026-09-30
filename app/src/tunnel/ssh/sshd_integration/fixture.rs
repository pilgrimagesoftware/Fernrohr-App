//! The `LocalSshd` fixture: a throwaway sshd + keypair spawned in a scratch
//! directory, torn down on drop, skipping when `sshd`/`ssh-keygen` aren't
//! installed - plus its shared spawn/lookup helpers used by every group of
//! tests in this module.

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command as StdCommand;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::tunnel::ssh::SshTunnelConfig;

/// A throwaway sshd + keypair rooted in a scratch directory, torn down on drop.
pub(super) struct LocalSshd {
    pub(super) dir: PathBuf,
    sshd: std::process::Child,
    pub(super) port: u16,
    pub(super) client_key: PathBuf,
    pub(super) known_hosts: PathBuf,
}

impl LocalSshd {
    pub(super) fn spawn() -> Option<Self> {
        Self::spawn_with_client_key(None)
    }

    /// Spawns a throwaway sshd, optionally reusing a client key generated for
    /// another instance (rather than minting a fresh one) so one client identity
    /// authenticates against every hop in a jump-host chain. Section 3.3.
    pub(super) fn spawn_with_client_key(shared_client_key: Option<&PathBuf>) -> Option<Self> {
        let sshd_path = which("sshd")?;
        let ssh_keygen = which("ssh-keygen")?;

        let dir = std::env::temp_dir().join(format!(
            "fernrohr-sshd-test-{}-{}",
            std::process::id(),
            uid()
        ));
        std::fs::create_dir_all(&dir).ok()?;

        let host_key = dir.join("host_key");
        let status = StdCommand::new(&ssh_keygen)
            .args(["-q", "-t", "ed25519", "-N", ""])
            .arg("-f")
            .arg(&host_key)
            .status()
            .ok()?;
        if !status.success() {
            return None;
        }

        let client_key = match shared_client_key {
            Some(existing) => existing.clone(),
            None => {
                let generated = dir.join("client_key");
                let status = StdCommand::new(&ssh_keygen)
                    .args(["-q", "-t", "ed25519", "-N", ""])
                    .arg("-f")
                    .arg(&generated)
                    .status()
                    .ok()?;
                if !status.success() {
                    return None;
                }
                generated
            }
        };

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

    pub(super) fn config(&self, remote_port: u16, local_port: u16) -> SshTunnelConfig {
        self.config_via(&[], remote_port, local_port, &self.known_hosts, None)
    }

    /// Builds a config that reaches this instance's sshd through `jump_hosts` (each
    /// naming a `Host` alias resolved via `ssh_config_file`'s `-F` config), using
    /// `known_hosts` instead of this instance's own file so a merged file covering
    /// every hop can be supplied. Section 3.3.
    pub(super) fn config_via(
        &self,
        jump_hosts: &[String],
        remote_port: u16,
        local_port: u16,
        known_hosts: &std::path::Path,
        ssh_config_file: Option<&PathBuf>,
    ) -> SshTunnelConfig {
        SshTunnelConfig {
            bastion_user: std::env::var("USER").unwrap(),
            bastion_host: "127.0.0.1".to_string(),
            bastion_port: self.port,
            jump_hosts: jump_hosts.to_vec(),
            remote_host: "127.0.0.1".to_string(),
            remote_port,
            local_port,
            identity_file: Some(self.client_key.clone()),
            known_hosts_file: Some(known_hosts.to_path_buf()),
            ssh_config_file: ssh_config_file.cloned(),
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

/// Distinguishes multiple `LocalSshd` scratch dirs spawned within the same test
/// process (e.g. a jump host and a target in a chain), since `process::id()` alone
/// is the same for both.
pub(super) fn uid() -> u32 {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub(super) fn which(bin: &str) -> Option<PathBuf> {
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
pub(super) async fn spawn_echo_server() -> u16 {
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
