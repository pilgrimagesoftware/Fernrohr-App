//! Task 4.3 coverage: `test_connection` reaches a real local sshd and reports
//! success or failure, all without a `ForwardRegistry` in scope at all - the
//! function's own signature makes acquiring a forward structurally impossible
//! here.

use std::net::TcpListener;
use std::process::Command as StdCommand;

use crate::tunnel::ssh::SshTunnelConfig;

use super::fixture::{LocalSshd, which};

/// Task 4.3: `test_connection` reaches a real local sshd and reports success -
/// without a `ForwardRegistry` in scope at all, since the function's own signature
/// (`&SshTunnelConfig` in, `Result<(), String>` out - no `&mut App`, no registry) makes
/// it structurally impossible for a call here to acquire a forward.
#[tokio::test]
async fn test_connection_succeeds_against_a_local_sshd() {
    let Some(sshd) = LocalSshd::spawn() else {
        eprintln!("skipping: no local sshd/ssh-keygen available to spawn a test bastion");
        return;
    };

    // remote_host/remote_port/local_port are unused by `test_args` (no `-N -L` is
    // ever built), so dummy values stand in for the forward target this test never
    // needs.
    let config = sshd.config(0, 0);

    super::super::test_connection(&config)
        .await
        .expect("ssh should reach the local test bastion and run `true`");
}

/// Task 4.3: a closed port reports the SSH client's own failure rather than hanging
/// or panicking - `ConnectTimeout=15` in `test_args` bounds the wait.
#[tokio::test]
async fn test_connection_reports_failure_against_a_closed_port() {
    let Some(ssh_keygen) = which("ssh-keygen") else {
        eprintln!("skipping: no ssh-keygen available to generate a throwaway key");
        return;
    };

    let dir = std::env::temp_dir().join(format!(
        "fernrohr-test-connection-closed-port-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let client_key = dir.join("client_key");
    StdCommand::new(&ssh_keygen)
        .args(["-q", "-t", "ed25519", "-N", ""])
        .arg("-f")
        .arg(&client_key)
        .status()
        .unwrap();

    // Bind then immediately drop: the port is free but nothing answers, giving a
    // real, fast connection-refused rather than a timed-out hang.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    // The connection is refused before authentication, so the user never matters -
    // and `USER` is unset in minimal containers.
    let config = SshTunnelConfig {
        bastion_user: "fernrohr-test".to_string(),
        bastion_host: "127.0.0.1".to_string(),
        bastion_port: port,
        jump_hosts: Vec::new(),
        remote_host: "127.0.0.1".to_string(),
        remote_port: 0,
        local_port: 0,
        identity_file: Some(client_key),
        known_hosts_file: None,
        ssh_config_file: None,
    };

    let result = super::super::test_connection(&config).await;

    assert!(result.is_err(), "a closed port must report a failure");
    let _ = std::fs::remove_dir_all(&dir);
}
