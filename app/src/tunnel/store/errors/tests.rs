//! `command-tunnels` 1.3: a command tunnel is refused, field by field, for an empty
//! command, unbalanced quotes, no way to know its port, a port outside 1-65535, or a
//! timeout that isn't positive - through the store's own save path.

use crate::config::tunnels::{CommandTunnelConfig, TunnelConfig, TunnelKind};
use crate::tunnel::store::test_support::temp_config_path;
use crate::tunnel::store::{TunnelFieldError, TunnelStore, TunnelStoreError};

fn command_tunnel(command_line: &str, local_port: Option<u16>, timeout: u64) -> TunnelConfig {
    TunnelConfig {
        name: "QA IAP".into(),
        kind: TunnelKind::Command,
        command: CommandTunnelConfig {
            command_line: command_line.into(),
            local_port,
            startup_timeout_secs: timeout,
            ..CommandTunnelConfig::default()
        },
        ..TunnelConfig::default()
    }
}

fn rejected(tunnel: TunnelConfig) -> Vec<TunnelFieldError> {
    let store = TunnelStore::new(temp_config_path());
    match store.create("qa-iap", tunnel, None) {
        Err(TunnelStoreError::Invalid(errors)) => errors,
        other => panic!("expected the tunnel to be refused, got {other:?}"),
    }
}

#[test]
fn a_valid_command_tunnel_saves_with_its_ssh_fields_empty() {
    let store = TunnelStore::new(temp_config_path());
    store
        .create(
            "qa-iap",
            command_tunnel("ssh -N -L{port}:127.0.0.1:8888 bastion", None, 30),
            None,
        )
        .expect("saves: the SSH fields aren't checked for a command tunnel");
    store
        .create(
            "fixed",
            command_tunnel("ssh -N -L8888:127.0.0.1:8888 bastion", Some(8888), 30),
            None,
        )
        .expect("a fixed port stands in for {port}");
}

#[test]
fn an_empty_command_is_refused() {
    assert_eq!(
        rejected(command_tunnel("  ", None, 30)),
        [TunnelFieldError::EmptyCommand]
    );
}

#[test]
fn unbalanced_quotes_are_refused() {
    assert_eq!(
        rejected(command_tunnel("ssh -L{port}:x:1 'host", None, 30)),
        [TunnelFieldError::UnbalancedQuotes]
    );
}

#[test]
fn no_placeholder_and_no_fixed_port_is_refused() {
    assert_eq!(
        rejected(command_tunnel(
            "ssh -N -L8888:127.0.0.1:8888 bastion",
            None,
            30
        )),
        [TunnelFieldError::NoPortPlaceholder]
    );
}

#[test]
fn a_fixed_port_of_zero_is_refused() {
    assert_eq!(
        rejected(command_tunnel("ssh -L{port}:x:1 host", Some(0), 30)),
        [TunnelFieldError::InvalidLocalPort]
    );
}

#[test]
fn a_zero_timeout_is_refused() {
    assert_eq!(
        rejected(command_tunnel("ssh -L{port}:x:1 host", None, 0)),
        [TunnelFieldError::InvalidTimeout]
    );
}

#[test]
fn an_ssh_tunnel_is_still_checked_as_ssh() {
    let tunnel = TunnelConfig {
        kind: TunnelKind::Ssh,
        ..command_tunnel("ssh -L{port}:x:1 host", None, 30)
    };
    assert_eq!(
        rejected(tunnel),
        [TunnelFieldError::EmptyHost, TunnelFieldError::EmptyUser]
    );
}
