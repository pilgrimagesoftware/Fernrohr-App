//! Exercises `SshTransport` against a real local `sshd`, not a mock - the closest thing
//! to "a local sshd container" this machine can run without Docker. Proves the forward
//! actually comes `Up` and proxies bytes, not just that the child process spawns.

use super::*;
use crate::forward_supervisor::ForwardTransport;
use std::net::TcpListener;
use std::process::Command as StdCommand;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

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
        Self::spawn_with_client_key(None)
    }

    /// Spawns a throwaway sshd, optionally reusing a client key generated for
    /// another instance (rather than minting a fresh one) so one client identity
    /// authenticates against every hop in a jump-host chain. Section 3.3.
    fn spawn_with_client_key(shared_client_key: Option<&PathBuf>) -> Option<Self> {
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

    fn config(&self, remote_port: u16, local_port: u16) -> SshTunnelConfig {
        self.config_via(&[], remote_port, local_port, &self.known_hosts, None)
    }

    /// Builds a config that reaches this instance's sshd through `jump_hosts` (each
    /// naming a `Host` alias resolved via `ssh_config_file`'s `-F` config), using
    /// `known_hosts` instead of this instance's own file so a merged file covering
    /// every hop can be supplied. Section 3.3.
    fn config_via(
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
fn uid() -> u32 {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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

/// True if any process is still alive in the process group rooted at `pid` (the
/// direct `ssh` child's pid, which is also its own pgid since `connect` spawns it
/// with `process_group(0)`). A `-J` jump-host hop inherits that group rather than
/// calling `setpgid` itself, so this catches it too.
fn process_group_has_members(pid: u32) -> bool {
    StdCommand::new("pgrep")
        .arg("-g")
        .arg(pid.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
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

/// Section 3.3: proves `-J` chains actually establish the forward through the jump
/// host in order, not just that the flag is present in the argument list (that's
/// the unit test above). Two independent local sshd instances stand in for "two
/// chained local sshd containers" - jump hops to target, target's sshd carries the
/// `-L` forward to the echo server.
#[tokio::test]
async fn ssh_tunnel_forwards_through_a_jump_host_chain() {
    let Some(jump) = LocalSshd::spawn() else {
        eprintln!("skipping: no local sshd/ssh-keygen available to spawn a test jump host");
        return;
    };
    let Some(target) = LocalSshd::spawn_with_client_key(Some(&jump.client_key)) else {
        eprintln!("skipping: failed to spawn the chained target sshd");
        return;
    };

    // The outer ssh's own -o UserKnownHostsFile/-i only cover the direct (target)
    // connection: -J hops are resolved by a *separate* ssh subprocess that
    // re-reads ~/.ssh/config from the real user's home directory, ignoring the
    // outer process's -o flags entirely (confirmed with -vvv against these two
    // throwaway sshd instances). -F, unlike $HOME, is passed through to that
    // nested subprocess, so the jump hop is trusted via a scratch -F config that
    // aliases it to the shared client key and known_hosts.
    let jump_user = std::env::var("USER").unwrap();
    let ssh_config_file = jump.dir.join("jump_ssh_config");
    std::fs::write(
        &ssh_config_file,
        format!(
            "Host jumphop\n\
             \tHostName 127.0.0.1\n\
             \tPort {}\n\
             \tUser {jump_user}\n\
             \tIdentityFile {}\n\
             \tUserKnownHostsFile {}\n\
             \tStrictHostKeyChecking yes\n\
             \tBatchMode yes\n",
            jump.port,
            jump.client_key.display(),
            jump.known_hosts.display(),
        ),
    )
    .unwrap();

    let echo_port = spawn_echo_server().await;
    let local_port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();

    let config = target.config_via(
        &["jumphop".to_string()],
        echo_port,
        local_port,
        &target.known_hosts,
        Some(&ssh_config_file),
    );
    let mut transport = SshTransport::new(config);
    transport
        .connect()
        .await
        .expect("ssh forward should come up through the jump host chain");

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

    stream
        .write_all(b"hello through the jump chain")
        .await
        .unwrap();
    let mut buf = [0u8; 64];
    let n = stream.read(&mut buf).await.unwrap();
    assert_eq!(&buf[..n], b"hello through the jump chain");

    transport
        .health_check()
        .await
        .expect("ssh child should still be running");
}

/// Section 3.4: dropping the transport must kill the *whole* process group, not just
/// the direct `ssh` child - otherwise a `-J` hop (a separate `ssh` subprocess that
/// inherits the group rather than detaching) survives as an orphan. Establishes a
/// forward through a jump-host chain exactly like the test above, then drops the
/// transport and asserts `pgrep -g <pid>` finds nobody left in that group.
#[tokio::test]
async fn dropping_the_transport_kills_the_whole_process_group_including_the_jump_hop() {
    let Some(jump) = LocalSshd::spawn() else {
        eprintln!("skipping: no local sshd/ssh-keygen available to spawn a test jump host");
        return;
    };
    let Some(target) = LocalSshd::spawn_with_client_key(Some(&jump.client_key)) else {
        eprintln!("skipping: failed to spawn the chained target sshd");
        return;
    };
    let Some(_pgrep) = which("pgrep") else {
        eprintln!("skipping: no pgrep available to inspect the process group");
        return;
    };

    let jump_user = std::env::var("USER").unwrap();
    let ssh_config_file = jump.dir.join("jump_ssh_config");
    std::fs::write(
        &ssh_config_file,
        format!(
            "Host jumphop\n\
             \tHostName 127.0.0.1\n\
             \tPort {}\n\
             \tUser {jump_user}\n\
             \tIdentityFile {}\n\
             \tUserKnownHostsFile {}\n\
             \tStrictHostKeyChecking yes\n\
             \tBatchMode yes\n",
            jump.port,
            jump.client_key.display(),
            jump.known_hosts.display(),
        ),
    )
    .unwrap();

    let echo_port = spawn_echo_server().await;
    let local_port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();

    let config = target.config_via(
        &["jumphop".to_string()],
        echo_port,
        local_port,
        &target.known_hosts,
        Some(&ssh_config_file),
    );
    let mut transport = SshTransport::new(config);
    transport
        .connect()
        .await
        .expect("ssh forward should come up through the jump host chain");
    let pid = transport
        .child_pid()
        .expect("connect should have recorded the spawned child's pid");

    // Give the -J hop's own ssh subprocess a moment to actually be running under the
    // same group before we check for it - connect() only proves the local port is
    // accepting connections, not that the hop process has finished forking.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        process_group_has_members(pid),
        "expected ssh (and its jump-host hop) to still be running before drop"
    );

    drop(transport);
    // kill_process_group shells out to `kill`; give the signal a moment to land.
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert!(
        !process_group_has_members(pid),
        "no member of ssh's process group (including the -J hop) should survive Drop"
    );
}
