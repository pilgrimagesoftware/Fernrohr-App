//! Shared test helpers for the store's split test modules: a scratch
//! `tunnels.toml` path, a process-unique id generator, and a sample tunnel config.

// Named imports rather than `use super::*`: keeps this module's own
// dependency list explicit for the test modules that import from it.

use crate::config::tunnels::TunnelConfig;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) fn temp_config_path() -> PathBuf {
    crate::util::test_paths::temp_path("tunnel-store")
}

/// A counter-derived id, unique per test run within this process. Tests
/// that write a real keychain secret use this instead of a fixed literal
/// ("stable-id", "prod-bastion") - a fixed account name means a run that
/// panics or is killed before its own cleanup leaves a real keychain
/// entry behind, and the *next* run then either collides with it or
/// (for update_renames_..., previously the actual cause of a machine-wide
/// test hang) silently asserts against stale state from a prior run
/// instead of its own.
pub(super) fn next_id() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

pub(super) fn sample_tunnel(name: &str) -> TunnelConfig {
    TunnelConfig {
        name: name.to_string(),
        bastion_user: "ops".into(),
        bastion_host: "bastion.example.com".into(),
        bastion_port: 22,
        jump_hosts: Vec::new(),
        auth: crate::config::tunnels::TunnelAuth::default(),
        ..Default::default()
    }
}
