//! Forwarding coverage: proves `SshTransport` and the `SshTunnel` wrapper
//! around it both bring a real TCP forward `Up` through a local sshd and
//! proxy bytes end to end, not just that they spawn.

use std::net::TcpListener;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::forward::supervisor::ForwardTransport;
use crate::tunnel::ssh::{SshTransport, SshTunnel};

use super::fixture::{LocalSshd, spawn_echo_server};

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

/// Section 6.2: proves `SshTunnel` - the `ManagedForward` wrapper the connect path
/// acquires from `ForwardRegistry` - actually reaches `ForwardState::Up` and proxies
/// traffic against a real local sshd, not just that its glue compiles against a fake.
/// `SshTransport`/`ForwardSupervisor` already have their own coverage above and in
/// `forward_supervisor.rs`; this test is only about `SshTunnel` composing them.
#[tokio::test]
async fn ssh_tunnel_wrapper_reaches_up_and_proxies_through_local_sshd() {
    use crate::forward::managed::{ForwardState, ManagedForward as _};
    use crate::forward::supervisor::{BackoffPolicy, SupervisorOptions};
    use std::time::Duration as StdDuration;

    let Some(sshd) = LocalSshd::spawn() else {
        eprintln!("skipping: no local sshd/ssh-keygen available to spawn a test bastion");
        return;
    };

    let echo_port = spawn_echo_server().await;
    let local_addr: std::net::SocketAddr = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();

    let tunnel = SshTunnel::spawn(
        &tokio::runtime::Handle::current(),
        sshd.config(echo_port, local_addr.port()),
        None,
        local_addr,
        SupervisorOptions {
            health_check_interval: StdDuration::from_secs(30),
            backoff: BackoffPolicy {
                initial: StdDuration::from_millis(100),
                max: StdDuration::from_secs(1),
            },
        },
    );

    let mut state = tunnel.state();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while *state.borrow() != ForwardState::Up {
        tokio::select! {
            result = state.changed() => result.expect("supervisor task should not exit while the tunnel is held"),
            _ = tokio::time::sleep_until(deadline) => panic!("tunnel never reached Up"),
        }
    }
    assert_eq!(tunnel.local_addr(), local_addr);

    let mut stream = tokio::net::TcpStream::connect(local_addr)
        .await
        .expect("forwarded local port should accept connections once Up");
    stream.write_all(b"hello via SshTunnel").await.unwrap();
    let mut buf = [0u8; 64];
    let n = stream.read(&mut buf).await.unwrap();
    assert_eq!(&buf[..n], b"hello via SshTunnel");
}
