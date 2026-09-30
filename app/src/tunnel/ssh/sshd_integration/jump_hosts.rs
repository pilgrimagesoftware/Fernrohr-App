//! Section 3.3 coverage: `-J` jump-host chains actually establish the forward
//! through the jump host in order, using two independent local sshd instances
//! to stand in for "two chained local sshd containers".

use std::net::TcpListener;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::forward::supervisor::ForwardTransport;
use crate::tunnel::ssh::SshTransport;

use super::fixture::{LocalSshd, spawn_echo_server};

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
