//! `command-tunnels` 4.2: each row's kind badge and second line. That the window
//! draws the badge is `window`'s tests'.

use super::{kind_label, tunnel_summary};
use crate::config::tunnels::{CommandTunnelConfig, CommandTunnelMode, TunnelConfig, TunnelKind};

fn ssh_tunnel() -> TunnelConfig {
    TunnelConfig {
        name: "QA bastion".into(),
        bastion_user: "ops".into(),
        bastion_host: "bastion.example.com".into(),
        ..TunnelConfig::default()
    }
}

fn command_tunnel(command_line: &str) -> TunnelConfig {
    TunnelConfig {
        name: "QA IAP".into(),
        kind: TunnelKind::Command,
        command: CommandTunnelConfig {
            command_line: command_line.into(),
            ..CommandTunnelConfig::default()
        },
        ..TunnelConfig::default()
    }
}

#[test]
fn a_row_says_which_kind_and_what_it_runs() {
    assert_eq!(kind_label(TunnelKind::Ssh), "SSH");
    assert_eq!(kind_label(TunnelKind::Command), "Command");
    assert_eq!(tunnel_summary(&ssh_tunnel()), "ops@bastion.example.com:22");
    assert_eq!(
        tunnel_summary(&command_tunnel("ssh -N -L{port}:127.0.0.1:8888 b")),
        "ssh -N -L{port}:127.0.0.1:8888 b (proxy)"
    );
    let mut forward = command_tunnel("gcloud compute ssh <host> \\\n  -- -N -L{port}:x:1");
    forward.command.mode = CommandTunnelMode::Forward;
    assert_eq!(
        tunnel_summary(&forward),
        "gcloud compute ssh <host>\u{2026} (forward)",
        "the first line, its continuation backslash dropped, marked as shortened"
    );
}
