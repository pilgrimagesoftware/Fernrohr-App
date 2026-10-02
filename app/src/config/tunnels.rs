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
    pub bastion_user: String,
    pub bastion_host: String,
    pub bastion_port: u16,
    /// Extra `user@host[:port]` hops for `-J`, nearest-to-target last.
    pub jump_hosts: Vec<String>,
    /// How to authenticate to the bastion. A non-secret marker only - the actual key
    /// material for `KeychainKey` lives in the OS keychain (`tunnel::secrets`), keyed
    /// by tunnel id, never here.
    pub auth: TunnelAuth,
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            bastion_user: String::new(),
            bastion_host: String::new(),
            bastion_port: 22,
            jump_hosts: Vec::new(),
            auth: TunnelAuth::default(),
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
