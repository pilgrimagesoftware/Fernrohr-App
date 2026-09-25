//! Section 5.1 of the tunnel-subsystem change: the `tunnels.toml` schema.
//!
//! Holds tunnel definitions and `context -> tunnel id` bindings. Deliberately carries
//! no secret material - a tunnel's private key lives in the OS keychain, keyed by
//! tunnel id (section 5.2), never in this file. `no_secret_fields` below guards against
//! that boundary eroding as fields get added.
// UNWIRED(#3): section 5.3's tunnel CRUD UI and section 6's context binding are the
// first real callers; today only this module's own tests build one.
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
    pub remote_host: String,
    pub remote_port: u16,
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            bastion_user: String::new(),
            bastion_host: String::new(),
            bastion_port: 22,
            jump_hosts: Vec::new(),
            remote_host: String::new(),
            remote_port: 0,
        }
    }
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
                remote_host: "10.0.1.5".into(),
                remote_port: 6443,
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
