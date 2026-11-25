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

/// Security pass: `--` ends ssh's options before the destination, so a user
/// that starts with `-` can't be read as one (`-oProxyCommand=...`) - in the
/// tunnel's argv and in the Test button's.
#[test]
fn a_dash_user_stays_the_destination() {
    let config = SshTunnelConfig {
        bastion_user: "-oProxyCommand=false".to_string(),
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
    let target = "-oProxyCommand=false@bastion.example.com".to_string();

    let args = config.args();
    assert_eq!(args[args.len() - 2..], ["--".to_string(), target.clone()]);
    let test_args = config.test_args();
    assert_eq!(
        test_args[test_args.len() - 3..],
        ["--".to_string(), target, "true".to_string()]
    );
}

/// Review follow-up: an option-like `-J` hop saved before the editor checked
/// (or edited by hand) never reaches `ssh`. A dash inside a name is fine.
#[test]
fn an_option_like_jump_host_is_refused_before_spawning() {
    assert!(is_option_like_jump_host("-oProxyCommand=false"));
    assert!(is_option_like_jump_host(" ops@-hop:22"));
    assert!(!is_option_like_jump_host("ops@hop-1.example.com:2222"));
    assert!(!is_option_like_jump_host("my-user@hop"));

    let config = SshTunnelConfig {
        bastion_user: "ops".to_string(),
        bastion_host: "bastion.example.com".to_string(),
        bastion_port: 22,
        jump_hosts: vec!["-oProxyCommand=false".to_string()],
        remote_host: "10.0.0.5".to_string(),
        remote_port: 6443,
        local_port: 40000,
        identity_file: None,
        known_hosts_file: None,
        ssh_config_file: None,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let error = runtime
        .block_on(test_connection(&config))
        .expect_err("refused");
    assert!(error.contains("jump host"), "{error}");
}
