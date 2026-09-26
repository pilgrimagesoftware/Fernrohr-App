//! Unit tests for `SshTunnelConfig::args()` and `classify_exit_failure`. No child
//! process spawned here - `sshd_integration.rs` covers the real `ssh` process tree.

use super::*;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

#[cfg(unix)]
fn fake_exit_status(code: i32) -> ExitStatus {
    ExitStatus::from_raw(code << 8)
}

#[cfg(unix)]
#[test]
fn auth_failure_and_bind_failure_produce_distinct_reasons() {
    let auth_reason = classify_exit_failure(
        fake_exit_status(255),
        "alice@bastion: Permission denied (publickey).",
    );
    let bind_reason = classify_exit_failure(
        fake_exit_status(255),
        "channel_setup_fwd_listener_tcpip: cannot listen to port: 40000",
    );

    assert!(auth_reason.contains("authentication failed"));
    assert!(bind_reason.contains("failed to bind"));
    assert_ne!(auth_reason, bind_reason);
}

#[cfg(unix)]
#[test]
fn unrecognized_stderr_falls_back_to_exit_status() {
    let reason = classify_exit_failure(fake_exit_status(1), "");
    assert!(reason.contains("exited before the forward came up"));
}

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
        ssh_config_file: None,
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
        ssh_config_file: None,
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
        ssh_config_file: None,
    };

    let args = config.args();
    let jump_index = args.iter().position(|a| a == "-J").unwrap();
    assert_eq!(args[jump_index + 1], "bob@hop1,carol@hop2");
}
