//! Section 5.1 of the tunnel-subsystem change: the `tunnels.toml` schema.
//!
//! Holds tunnel definitions and `context -> tunnel id` bindings. Deliberately carries
//! no secret material - a tunnel's private key lives in the OS keychain, keyed by
//! tunnel id (section 5.2), never in this file. `no_secret_fields` below guards against
//! that boundary eroding as fields get added.
//!
//! Section 1.1 of `tunnel-management-ui` dropped `remote_host`/`remote_port`: a tunnel
//! now describes only how to reach a bastion, and the forward target is derived from
//! the connecting context's kubeconfig `server:` URL (`k8s::cluster::kubeconfig::
//! server_for_context`). No `deny_unknown_fields` here, so a `tunnels.toml` written by
//! an older build still parses with those two fields simply ignored; the next save
//! omits them.
//!
//! `command-tunnels` adds a second kind: a tunnel is an SSH tunnel (the fields above,
//! still at the top level) or a command tunnel (the `command` table). Both sets of
//! fields are kept whatever the kind, so a file written before kinds existed loads as
//! SSH unchanged, and flipping a tunnel's kind in the editor and back loses nothing.
//!
//! `manual-confirmation-tunnels` adds a third: a manual tunnel starts nothing, and
//! holds a bound context's connection until the user confirms the network path they
//! bring up by hand (a VPN) is there. Its settings are the `manual` table, kept the
//! same way.
// UNWIRED(#3): `tunnel_store::TunnelStore` (section 5.3) is the first real caller;
// section 6's context binding UI is the first caller of `TunnelStore` itself.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TunnelsConfig {
    /// Keyed by tunnel id.
    pub tunnels: BTreeMap<String, TunnelConfig>,
    /// Cluster context name -> tunnel id. A context binds to zero or one tunnel;
    /// many contexts may bind to the same tunnel (enforced at the section 6.1 UI/store
    /// layer, not by this schema).
    pub context_bindings: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TunnelConfig {
    pub name: String,
    /// Which kind of tunnel this is; absent (an older file) means SSH.
    pub kind: TunnelKind,
    pub bastion_user: String,
    pub bastion_host: String,
    pub bastion_port: u16,
    /// Extra `user@host[:port]` hops for `-J`, nearest-to-target last.
    pub jump_hosts: Vec<String>,
    /// How to authenticate to the bastion. A non-secret marker only - the actual key
    /// material for `KeychainKey` lives in the OS keychain (`tunnel::secrets`), keyed
    /// by tunnel id, never here.
    pub auth: TunnelAuth,
    /// A command tunnel's settings - kept, but unused, while `kind` is another.
    pub command: CommandTunnelConfig,
    /// A manual tunnel's settings - kept, but unused, while `kind` is another.
    pub manual: ManualTunnelConfig,
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: TunnelKind::default(),
            bastion_user: String::new(),
            bastion_host: String::new(),
            bastion_port: 22,
            jump_hosts: Vec::new(),
            auth: TunnelAuth::default(),
            command: CommandTunnelConfig::default(),
            manual: ManualTunnelConfig::default(),
        }
    }
}

/// What starts a tunnel: Fernrohr's own `ssh -N -L` to a bastion, a command the user
/// supplies, or - for a manual tunnel - the user, by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelKind {
    #[default]
    Ssh,
    Command,
    /// Nothing Fernrohr starts: the user brings the network path up themselves and
    /// confirms it.
    Manual,
}

/// What a command tunnel's local port offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandTunnelMode {
    /// An HTTP proxy: a bound context keeps its real API server URL and sends its
    /// traffic through the proxy.
    #[default]
    Proxy,
    /// A direct path to the API server: a bound context is rewritten to the loopback
    /// port, as for an SSH tunnel.
    Forward,
}

/// A command tunnel: the command Fernrohr runs and supervises, and what its local port
/// is. Not secret - the editor says the command is stored in plain text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandTunnelConfig {
    /// Split with POSIX quoting, never run through a shell. `{port}` is replaced with
    /// the local port.
    pub command_line: String,
    pub mode: CommandTunnelMode,
    /// A port the command itself hard-codes; `None` allocates a free one at start.
    pub local_port: Option<u16>,
    pub startup_timeout_secs: u64,
}

impl Default for CommandTunnelConfig {
    fn default() -> Self {
        Self {
            command_line: String::new(),
            mode: CommandTunnelMode::default(),
            local_port: None,
            startup_timeout_secs: crate::consts::COMMAND_TUNNEL_STARTUP_TIMEOUT_SECS,
        }
    }
}

/// A manual tunnel: what the prompt tells the user to do, and whether a reachable API
/// server skips the prompt. Carries no command, host, port or credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ManualTunnelConfig {
    /// Shown in every prompt for this tunnel: "Connect the corporate VPN in the menu
    /// bar". `None` shows only the tunnel's name.
    pub message: Option<String>,
    /// Before prompting, try a short TCP connect to the context's API server, and
    /// treat the tunnel as confirmed when it answers.
    pub skip_when_reachable: bool,
}

impl Default for ManualTunnelConfig {
    fn default() -> Self {
        Self {
            message: None,
            skip_when_reachable: true,
        }
    }
}

/// A tunnel's authentication method. `SshConfig` (the default) leaves auth to the
/// system `ssh` client - its own `~/.ssh/config` and running agent; `KeychainKey`
/// marks that a private key for this tunnel is expected in the OS keychain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelAuth {
    #[default]
    SshConfig,
    KeychainKey,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TunnelsConfig {
        let mut tunnels = BTreeMap::new();
        tunnels.insert(
            "prod-bastion".to_string(),
            TunnelConfig {
                name: "Prod bastion".into(),
                bastion_user: "ops".into(),
                bastion_host: "bastion.example.com".into(),
                bastion_port: 22,
                jump_hosts: vec!["ops@hop1.example.com".into()],
                auth: TunnelAuth::KeychainKey,
                ..TunnelConfig::default()
            },
        );
        tunnels.insert(
            "qa-iap".to_string(),
            TunnelConfig {
                name: "QA IAP".into(),
                kind: TunnelKind::Command,
                command: CommandTunnelConfig {
                    command_line: "gcloud compute ssh <host> --tunnel-through-iap -- -N \\\n  -L{port}:127.0.0.1:8888".into(),
                    mode: CommandTunnelMode::Proxy,
                    local_port: Some(8888),
                    startup_timeout_secs: 45,
                },
                ..TunnelConfig::default()
            },
        );
        let mut context_bindings = BTreeMap::new();
        context_bindings.insert("prod".to_string(), "prod-bastion".to_string());
        TunnelsConfig {
            tunnels,
            context_bindings,
        }
    }

    #[test]
    fn round_trips_through_toml() {
        let config = sample();
        let text = toml::to_string(&config).unwrap();
        let parsed: TunnelsConfig = toml::from_str(&text).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn tolerates_unknown_fields() {
        let parsed: TunnelsConfig = toml::from_str("future_field = true\n").unwrap();
        assert_eq!(parsed, TunnelsConfig::default());
    }

    /// Tasks.md 1.1: a `tunnels.toml` written by a build that still had
    /// `remote_host`/`remote_port` on `TunnelConfig` must keep loading - the tunnel and
    /// its context binding intact - and the next save must omit both fields.
    #[test]
    fn legacy_remote_target_fields_are_ignored_on_load_and_dropped_on_save() {
        let legacy = r#"
[tunnels.prod-bastion]
name = "Prod bastion"
bastion_user = "ops"
bastion_host = "bastion.example.com"
bastion_port = 22
jump_hosts = []
remote_host = "10.0.1.5"
remote_port = 6443

[context_bindings]
prod = "prod-bastion"
"#;

        let parsed: TunnelsConfig = toml::from_str(legacy).unwrap();

        let tunnel = parsed.tunnels.get("prod-bastion").expect("tunnel loads");
        assert_eq!(tunnel.bastion_host, "bastion.example.com");
        assert_eq!(
            parsed.context_bindings.get("prod"),
            Some(&"prod-bastion".to_string())
        );

        let saved = toml::to_string(&parsed).unwrap();
        assert!(
            !saved.contains("remote_host"),
            "remote_host must not survive a save"
        );
        assert!(
            !saved.contains("remote_port"),
            "remote_port must not survive a save"
        );
    }

    /// `command-tunnels` 1.1: a file written before kinds existed loads every tunnel
    /// as SSH, with its fields and binding unchanged.
    #[test]
    fn a_file_without_kinds_loads_as_ssh() {
        let before = r#"
[tunnels.prod-bastion]
name = "Prod bastion"
bastion_user = "ops"
bastion_host = "bastion.example.com"
bastion_port = 2222
jump_hosts = []
auth = "keychain_key"

[context_bindings]
prod = "prod-bastion"
"#;
        let parsed: TunnelsConfig = toml::from_str(before).unwrap();
        let tunnel = parsed.tunnels.get("prod-bastion").expect("tunnel loads");
        assert_eq!(tunnel.kind, TunnelKind::Ssh);
        assert_eq!(tunnel.bastion_port, 2222);
        assert_eq!(tunnel.auth, TunnelAuth::KeychainKey);
        assert_eq!(tunnel.command, CommandTunnelConfig::default());
        assert_eq!(
            parsed.context_bindings.get("prod").map(String::as_str),
            Some("prod-bastion")
        );
    }

    /// `command-tunnels` 1.1: a command tunnel's command line, mode, fixed port and
    /// timeout survive a save and a load.
    #[test]
    fn a_command_tunnel_round_trips() {
        let config = sample();
        let text = toml::to_string(&config).unwrap();
        let parsed: TunnelsConfig = toml::from_str(&text).unwrap();
        let tunnel = &parsed.tunnels["qa-iap"];
        assert_eq!(tunnel.kind, TunnelKind::Command);
        assert_eq!(tunnel.command, config.tunnels["qa-iap"].command);
        let unset: CommandTunnelConfig = toml::from_str("command_line = \"x {port}\"").unwrap();
        assert_eq!(unset.mode, CommandTunnelMode::Proxy, "proxy is the default");
        assert_eq!(unset.local_port, None);
        assert_eq!(unset.startup_timeout_secs, 30);
    }

    /// `manual-confirmation-tunnels` 1.1: a manual tunnel's message and its
    /// skip-when-reachable setting survive a save and a load.
    #[test]
    fn a_manual_tunnel_round_trips() {
        let mut config = sample();
        config.tunnels.insert(
            "corp-vpn".to_string(),
            TunnelConfig {
                name: "corp-vpn".into(),
                kind: TunnelKind::Manual,
                manual: ManualTunnelConfig {
                    message: Some("Connect the corporate VPN in the menu bar".into()),
                    skip_when_reachable: false,
                },
                ..TunnelConfig::default()
            },
        );
        let text = toml::to_string(&config).unwrap();
        assert!(text.contains("kind = \"manual\""), "{text}");
        let parsed: TunnelsConfig = toml::from_str(&text).unwrap();
        assert_eq!(parsed, config);
    }

    /// `manual-confirmation-tunnels` 1.1: a file written before the manual kind - no
    /// `manual` table anywhere - loads with the defaults: no message, and the
    /// reachability shortcut on.
    #[test]
    fn a_file_without_a_manual_table_loads_with_its_defaults() {
        let before = r#"
[tunnels.qa-iap]
name = "QA IAP"
kind = "command"

[tunnels.qa-iap.command]
command_line = "gcloud start-iap-tunnel x 443 --local-host-port=localhost:{port}"
"#;
        let parsed: TunnelsConfig = toml::from_str(before).unwrap();
        let tunnel = &parsed.tunnels["qa-iap"];
        assert_eq!(tunnel.manual, ManualTunnelConfig::default());
        assert_eq!(tunnel.manual.message, None);
        assert!(tunnel.manual.skip_when_reachable);
        let unset: ManualTunnelConfig = toml::from_str("message = \"Up the VPN\"").unwrap();
        assert!(unset.skip_when_reachable, "on by default");
    }

    /// `manual-confirmation-tunnels` 1.1: switching kind keeps every other kind's
    /// settings - an SSH tunnel flipped to manual and back still has its bastion and
    /// command fields, and the manual fields stay once it is SSH again.
    #[test]
    fn switching_kind_keeps_every_kinds_settings() {
        let original = sample().tunnels["prod-bastion"].clone();
        let mut tunnel = original.clone();
        tunnel.kind = TunnelKind::Manual;
        tunnel.manual.message = Some("Up the VPN".into());
        let text = toml::to_string(&tunnel).unwrap();
        let mut back: TunnelConfig = toml::from_str(&text).unwrap();
        assert_eq!(back.bastion_host, original.bastion_host);
        assert_eq!(back.jump_hosts, original.jump_hosts);
        assert_eq!(back.auth, original.auth);
        assert_eq!(back.command, original.command);

        back.kind = TunnelKind::Ssh;
        let text = toml::to_string(&back).unwrap();
        let ssh: TunnelConfig = toml::from_str(&text).unwrap();
        assert_eq!(ssh.kind, TunnelKind::Ssh);
        assert_eq!(ssh.manual.message.as_deref(), Some("Up the VPN"));
        assert_eq!(ssh.bastion_host, original.bastion_host);
    }

    /// Guards the design intent documented on the module and struct: a tunnel's
    /// secret material must never round-trip through this file. Scans the serialized
    /// TOML's keys, not just field names, so a future field named e.g. `identity_file`
    /// or `password` fails this test the moment it starts serializing.
    #[test]
    fn no_secret_fields() {
        const BANNED_SUBSTRINGS: &[&str] = &[
            "password",
            "passphrase",
            "secret",
            "private_key",
            "identity_file",
            "key_material",
            "token",
        ];
        let text = toml::to_string(&sample()).unwrap();
        for line in text.lines() {
            let Some((key, _)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim().to_lowercase();
            for banned in BANNED_SUBSTRINGS {
                assert!(
                    !key.contains(banned),
                    "tunnels.toml key {key:?} contains banned substring {banned:?} - secret material belongs in the keychain, not this file"
                );
            }
        }
    }
}
