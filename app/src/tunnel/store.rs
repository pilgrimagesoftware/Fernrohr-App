//! Section 5.3 of the tunnel-subsystem change: tunnel CRUD composing section 5.1's
//! `tunnels.toml` schema with section 5.2's keychain wrapper.
//!
//! `TunnelStore` is the single place that keeps `tunnels.toml` and the keychain in
//! sync: creating or editing a tunnel writes non-secret fields to the config file and
//! the secret (if any) to the keychain in one call, and deleting a tunnel removes both
//! plus any `context -> tunnel id` bindings that pointed at it. Renaming is just an
//! edit - the tunnel id is stable and never derived from its display name.
//!
//! Section 6.1 adds `context -> tunnel id` binding on top of the same file: a context
//! binds to zero or one tunnel (`bind` overwrites any prior binding for that context)
//! and many contexts may share one tunnel (`bind` never checks who else points at
//! `tunnel_id`). Persistence is `tunnels.toml` itself, already loaded/saved by every
//! method here - there is nothing else to persist across a relaunch.
// UNWIRED(#3): section 6.2's connect-path integration is the first real caller.
#![allow(dead_code)]

use crate::config::{
    self,
    tunnels::{TunnelConfig, TunnelsConfig},
};
use crate::tunnel::secrets::TunnelSecretStore;
use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub enum TunnelStoreError {
    NotFound,
    AlreadyExists,
    /// `create`/`update` rejected the tunnel before writing it - one entry per
    /// offending field, so a caller (the section 4.2 editor) can mark each inline.
    Invalid(Vec<TunnelFieldError>),
    Secret(keyring::Error),
    Io(io::Error),
}

/// A single invalid field on a [`TunnelConfig`] passed to
/// [`TunnelStore::create`]/[`TunnelStore::update`]. `bastion_port` is a `u16`, so the
/// only out-of-range value in 1-65535 is `0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelFieldError {
    EmptyHost,
    EmptyUser,
    InvalidPort,
}

/// Collects every violation of tasks.md 1.2's validation rules (non-empty host and
/// user, port 1-65535). Empty, not `None`, when `tunnel` is valid.
fn validate(tunnel: &TunnelConfig) -> Vec<TunnelFieldError> {
    let mut errors = Vec::new();
    if tunnel.bastion_host.trim().is_empty() {
        errors.push(TunnelFieldError::EmptyHost);
    }
    if tunnel.bastion_user.trim().is_empty() {
        errors.push(TunnelFieldError::EmptyUser);
    }
    if tunnel.bastion_port == 0 {
        errors.push(TunnelFieldError::InvalidPort);
    }
    errors
}

impl From<keyring::Error> for TunnelStoreError {
    fn from(err: keyring::Error) -> Self {
        Self::Secret(err)
    }
}

impl From<io::Error> for TunnelStoreError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

pub struct TunnelStore {
    config_path: PathBuf,
    secrets: TunnelSecretStore,
}

impl TunnelStore {
    /// Test builds (`cfg(test)`) never touch the real OS keychain: every `TunnelStore`,
    /// whether built directly by this module's own tests or indirectly through the
    /// tunnel editor and connect-path code those tests exercise, gets
    /// [`TunnelSecretStore::new_in_memory_only`] instead of the real keychain-backed
    /// store, so a plain `cargo test` never triggers a macOS Keychain access prompt.
    /// Production (`cfg(not(test))`) behavior is unchanged.
    pub fn new(config_path: PathBuf) -> Self {
        #[cfg(test)]
        let secrets = TunnelSecretStore::new_in_memory_only();
        #[cfg(not(test))]
        let secrets = TunnelSecretStore::new();

        Self {
            config_path,
            secrets,
        }
    }

    pub fn list(&self) -> Vec<(String, TunnelConfig)> {
        let config: TunnelsConfig = config::load(&self.config_path);
        config.tunnels.into_iter().collect()
    }

    /// A single tunnel's non-secret config, if `id` exists.
    pub fn get(&self, id: &str) -> Option<TunnelConfig> {
        let config: TunnelsConfig = config::load(&self.config_path);
        config.tunnels.get(id).cloned()
    }

    /// The tunnel id `context` is bound to, if any - section 6.2's connect path uses
    /// this to decide whether to route a context's connection through a forward.
    pub fn binding_for(&self, context: &str) -> Option<String> {
        let config: TunnelsConfig = config::load(&self.config_path);
        config.context_bindings.get(context).cloned()
    }

    /// `tunnel_id`'s stored secret (private key), if any. Kept separate from
    /// [`Self::get`] since reading a secret means a keychain round trip.
    pub fn secret(&self, tunnel_id: &str) -> Result<Option<String>, TunnelStoreError> {
        Ok(self.secrets.read(tunnel_id)?)
    }

    /// Creates a new tunnel under `id`. Fails if `id` is already in use - use
    /// [`Self::update`] to edit or rename an existing tunnel.
    pub fn create(
        &self,
        id: &str,
        tunnel: TunnelConfig,
        secret: Option<&str>,
    ) -> Result<(), TunnelStoreError> {
        let errors = validate(&tunnel);
        if !errors.is_empty() {
            return Err(TunnelStoreError::Invalid(errors));
        }
        let mut config: TunnelsConfig = config::load(&self.config_path);
        if config.tunnels.contains_key(id) {
            return Err(TunnelStoreError::AlreadyExists);
        }
        if let Some(secret) = secret {
            self.secrets.save(id, secret)?;
        }
        config.tunnels.insert(id.to_string(), tunnel);
        config::save(&self.config_path, &config)?;
        Ok(())
    }

    /// Updates an existing tunnel's fields (including a rename via `tunnel.name`) and,
    /// when `secret` is `Some`, replaces its stored secret. Passing `None` leaves
    /// whatever secret (if any) is already stored untouched.
    pub fn update(
        &self,
        id: &str,
        tunnel: TunnelConfig,
        secret: Option<&str>,
    ) -> Result<(), TunnelStoreError> {
        let errors = validate(&tunnel);
        if !errors.is_empty() {
            return Err(TunnelStoreError::Invalid(errors));
        }
        let mut config: TunnelsConfig = config::load(&self.config_path);
        if !config.tunnels.contains_key(id) {
            return Err(TunnelStoreError::NotFound);
        }
        if let Some(secret) = secret {
            self.secrets.save(id, secret)?;
        }
        config.tunnels.insert(id.to_string(), tunnel);
        config::save(&self.config_path, &config)?;
        Ok(())
    }

    /// Deletes a tunnel: removes its `tunnels.toml` entry and keychain secret, and
    /// unbinds every context that pointed at it. Returns the names of the contexts
    /// that were unbound, so the caller can warn the user before or after the fact.
    pub fn delete(&self, id: &str) -> Result<Vec<String>, TunnelStoreError> {
        let mut config: TunnelsConfig = config::load(&self.config_path);
        if config.tunnels.remove(id).is_none() {
            return Err(TunnelStoreError::NotFound);
        }

        let unbound: Vec<String> = config
            .context_bindings
            .iter()
            .filter(|(_, tunnel_id)| tunnel_id.as_str() == id)
            .map(|(context, _)| context.clone())
            .collect();
        for context in &unbound {
            config.context_bindings.remove(context);
        }

        self.secrets.delete(id)?;
        config::save(&self.config_path, &config)?;
        Ok(unbound)
    }

    /// Lists every `context -> tunnel id` binding.
    pub fn bindings(&self) -> Vec<(String, String)> {
        let config: TunnelsConfig = config::load(&self.config_path);
        config.context_bindings.into_iter().collect()
    }

    /// Binds `context` to `tunnel_id`, overwriting any prior binding for that context.
    /// Fails if `tunnel_id` does not exist. Many contexts may bind to the same tunnel.
    pub fn bind(&self, context: &str, tunnel_id: &str) -> Result<(), TunnelStoreError> {
        let mut config: TunnelsConfig = config::load(&self.config_path);
        if !config.tunnels.contains_key(tunnel_id) {
            return Err(TunnelStoreError::NotFound);
        }
        config
            .context_bindings
            .insert(context.to_string(), tunnel_id.to_string());
        config::save(&self.config_path, &config)?;
        Ok(())
    }

    /// Removes `context`'s binding, if any. A no-op (not an error) if it was already
    /// unbound.
    pub fn unbind(&self, context: &str) -> Result<(), TunnelStoreError> {
        let mut config: TunnelsConfig = config::load(&self.config_path);
        config.context_bindings.remove(context);
        config::save(&self.config_path, &config)?;
        Ok(())
    }

    /// How many contexts are bound to each tunnel, for the section 4 Tunnels panel's
    /// read-only usage count. Every existing tunnel id is present, at `0` if unused.
    pub fn usage_counts(&self) -> BTreeMap<String, usize> {
        let config: TunnelsConfig = config::load(&self.config_path);
        let mut counts: BTreeMap<String, usize> =
            config.tunnels.keys().map(|id| (id.clone(), 0)).collect();
        for tunnel_id in config.context_bindings.values() {
            *counts.entry(tunnel_id.clone()).or_insert(0) += 1;
        }
        counts
    }

    /// Bindings whose context is absent from `existing_contexts` (the kubeconfig's own
    /// context list) - e.g. after a `kubectl config rename-context`. Each pair is
    /// `(context, tunnel_id)`.
    pub fn stale_bindings(&self, existing_contexts: &[String]) -> Vec<(String, String)> {
        let config: TunnelsConfig = config::load(&self.config_path);
        config
            .context_bindings
            .into_iter()
            .filter(|(context, _)| !existing_contexts.iter().any(|c| c == context))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_config_path() -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("fernrohr-tunnel-store-test-{}.toml", next_id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    /// A counter-derived id, unique per test run within this process. Tests
    /// that write a real keychain secret use this instead of a fixed literal
    /// ("stable-id", "prod-bastion") - a fixed account name means a run that
    /// panics or is killed before its own cleanup leaves a real keychain
    /// entry behind, and the *next* run then either collides with it or
    /// (for update_renames_..., previously the actual cause of a machine-wide
    /// test hang) silently asserts against stale state from a prior run
    /// instead of its own.
    fn next_id() -> u64 {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        COUNTER.fetch_add(1, Ordering::Relaxed)
    }

    fn sample_tunnel(name: &str) -> TunnelConfig {
        TunnelConfig {
            name: name.to_string(),
            bastion_user: "ops".into(),
            bastion_host: "bastion.example.com".into(),
            bastion_port: 22,
            jump_hosts: Vec::new(),
            auth: crate::config::tunnels::TunnelAuth::default(),
        }
    }

    #[test]
    fn create_writes_config_and_secret() {
        let id = format!("prod-bastion-{}", next_id());
        let store = TunnelStore::new(temp_config_path());
        store
            .create(&id, sample_tunnel("Prod"), Some("s3cr3t"))
            .unwrap();

        let listed = store.list();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0, id);
        assert_eq!(store.secrets.read(&id).unwrap(), Some("s3cr3t".to_string()));

        store.secrets.delete(&id).unwrap();
    }

    #[test]
    fn create_with_existing_id_fails() {
        let store = TunnelStore::new(temp_config_path());
        store.create("dup", sample_tunnel("First"), None).unwrap();
        let err = store
            .create("dup", sample_tunnel("Second"), None)
            .unwrap_err();
        assert!(matches!(err, TunnelStoreError::AlreadyExists));
    }

    #[test]
    fn update_renames_without_changing_id_or_stored_secret_when_none_passed() {
        let id = format!("stable-id-{}", next_id());
        let store = TunnelStore::new(temp_config_path());
        store
            .create(&id, sample_tunnel("Old Name"), Some("s3cr3t"))
            .unwrap();

        store.update(&id, sample_tunnel("New Name"), None).unwrap();

        let listed = store.list();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0, id);
        assert_eq!(listed[0].1.name, "New Name");
        assert_eq!(
            store.secrets.read(&id).unwrap(),
            Some("s3cr3t".to_string()),
            "update without a secret must not clear the previously stored one"
        );

        store.secrets.delete(&id).unwrap();
    }

    #[test]
    fn update_of_missing_id_fails() {
        let store = TunnelStore::new(temp_config_path());
        let err = store
            .update("missing", sample_tunnel("Name"), None)
            .unwrap_err();
        assert!(matches!(err, TunnelStoreError::NotFound));
    }

    /// The unbind cascade required by tasks.md 5.3: deleting a tunnel must unbind every
    /// context that pointed at it and report which ones, while leaving bindings to
    /// other tunnels untouched.
    #[test]
    fn delete_unbinds_every_context_pointing_at_the_tunnel() {
        let path = temp_config_path();
        let store = TunnelStore::new(path.clone());
        store
            .create("prod-bastion", sample_tunnel("Prod"), Some("s3cr3t"))
            .unwrap();
        store
            .create("staging-bastion", sample_tunnel("Staging"), None)
            .unwrap();

        let mut config: TunnelsConfig = config::load(&path);
        config
            .context_bindings
            .insert("prod-east".to_string(), "prod-bastion".to_string());
        config
            .context_bindings
            .insert("prod-west".to_string(), "prod-bastion".to_string());
        config
            .context_bindings
            .insert("staging".to_string(), "staging-bastion".to_string());
        config::save(&path, &config).unwrap();

        let mut unbound = store.delete("prod-bastion").unwrap();
        unbound.sort();
        assert_eq!(
            unbound,
            vec!["prod-east".to_string(), "prod-west".to_string()]
        );

        let after = store.list();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].0, "staging-bastion");

        let after_config: TunnelsConfig = config::load(&path);
        assert_eq!(
            after_config.context_bindings.get("staging"),
            Some(&"staging-bastion".to_string()),
            "an unrelated binding must survive the delete"
        );
        assert!(!after_config.context_bindings.contains_key("prod-east"));
        assert!(!after_config.context_bindings.contains_key("prod-west"));

        assert_eq!(store.secrets.read("prod-bastion").unwrap(), None);
    }

    #[test]
    fn delete_of_missing_id_fails() {
        let store = TunnelStore::new(temp_config_path());
        let err = store.delete("missing").unwrap_err();
        assert!(matches!(err, TunnelStoreError::NotFound));
    }

    /// Tasks.md 6.1's "bind and persist" scenario: a binding written by one `TunnelStore`
    /// is visible to a fresh one reading the same file, standing in for a relaunch.
    #[test]
    fn bind_persists_across_a_fresh_store_over_the_same_file() {
        let path = temp_config_path();
        let store = TunnelStore::new(path.clone());
        store
            .create("qa-bastion", sample_tunnel("QA"), None)
            .unwrap();
        store.bind("qa-1", "qa-bastion").unwrap();

        let reopened = TunnelStore::new(path);
        assert_eq!(
            reopened.bindings(),
            vec![("qa-1".to_string(), "qa-bastion".to_string())]
        );
    }

    /// Tasks.md 6.1's "shared tunnel" scenario: many contexts may point at one tunnel id.
    #[test]
    fn many_contexts_can_share_one_tunnel() {
        let store = TunnelStore::new(temp_config_path());
        store
            .create("qa-bastion", sample_tunnel("QA"), None)
            .unwrap();
        store.bind("qa-1", "qa-bastion").unwrap();
        store.bind("qa-2", "qa-bastion").unwrap();

        let mut bindings = store.bindings();
        bindings.sort();
        assert_eq!(
            bindings,
            vec![
                ("qa-1".to_string(), "qa-bastion".to_string()),
                ("qa-2".to_string(), "qa-bastion".to_string()),
            ]
        );
    }

    /// A context binds to at most one tunnel: rebinding overwrites, it never accumulates.
    #[test]
    fn rebinding_a_context_overwrites_its_prior_binding() {
        let store = TunnelStore::new(temp_config_path());
        store.create("a", sample_tunnel("A"), None).unwrap();
        store.create("b", sample_tunnel("B"), None).unwrap();
        store.bind("ctx", "a").unwrap();
        store.bind("ctx", "b").unwrap();

        assert_eq!(store.bindings(), vec![("ctx".to_string(), "b".to_string())]);
    }

    #[test]
    fn bind_to_a_missing_tunnel_fails() {
        let store = TunnelStore::new(temp_config_path());
        let err = store.bind("ctx", "missing").unwrap_err();
        assert!(matches!(err, TunnelStoreError::NotFound));
        assert!(store.bindings().is_empty());
    }

    /// Tasks.md 6.1's "unbind" scenario.
    #[test]
    fn unbind_removes_the_binding() {
        let store = TunnelStore::new(temp_config_path());
        store.create("a", sample_tunnel("A"), None).unwrap();
        store.bind("ctx", "a").unwrap();

        store.unbind("ctx").unwrap();

        assert!(store.bindings().is_empty());
    }

    #[test]
    fn unbind_of_an_unbound_context_is_a_no_op() {
        let store = TunnelStore::new(temp_config_path());
        store.unbind("never-bound").unwrap();
        assert!(store.bindings().is_empty());
    }

    /// Tasks.md 1.2: `create`/`update` reject an empty `bastion_host`, naming the field.
    #[test]
    fn create_rejects_an_empty_host() {
        let store = TunnelStore::new(temp_config_path());
        let mut tunnel = sample_tunnel("Bad");
        tunnel.bastion_host = String::new();

        let err = store.create("bad", tunnel, None).unwrap_err();

        assert!(matches!(
            err,
            TunnelStoreError::Invalid(errors) if errors == vec![TunnelFieldError::EmptyHost]
        ));
        assert!(store.list().is_empty(), "an invalid tunnel must not save");
    }

    /// Tasks.md 1.2: an empty `bastion_user` is rejected the same way.
    #[test]
    fn create_rejects_an_empty_user() {
        let store = TunnelStore::new(temp_config_path());
        let mut tunnel = sample_tunnel("Bad");
        tunnel.bastion_user = String::new();

        let err = store.create("bad", tunnel, None).unwrap_err();

        assert!(matches!(
            err,
            TunnelStoreError::Invalid(errors) if errors == vec![TunnelFieldError::EmptyUser]
        ));
    }

    /// Tasks.md 1.2: `bastion_port` must be in 1-65535; `u16` already caps the top, so
    /// only `0` is out of range.
    #[test]
    fn create_rejects_an_out_of_range_port() {
        let store = TunnelStore::new(temp_config_path());
        let mut tunnel = sample_tunnel("Bad");
        tunnel.bastion_port = 0;

        let err = store.create("bad", tunnel, None).unwrap_err();

        assert!(matches!(
            err,
            TunnelStoreError::Invalid(errors) if errors == vec![TunnelFieldError::InvalidPort]
        ));
    }

    /// `update` validates too, and rejects before touching the file.
    #[test]
    fn update_rejects_an_invalid_tunnel() {
        let store = TunnelStore::new(temp_config_path());
        store.create("a", sample_tunnel("A"), None).unwrap();
        let mut invalid = sample_tunnel("A renamed");
        invalid.bastion_user = String::new();

        let err = store.update("a", invalid, None).unwrap_err();

        assert!(matches!(
            err,
            TunnelStoreError::Invalid(errors) if errors == vec![TunnelFieldError::EmptyUser]
        ));
        assert_eq!(store.get("a").unwrap().name, "A");
    }

    /// Tasks.md 1.2: `usage_counts` reflects bindings shared across contexts, and
    /// includes an unused tunnel at `0` rather than omitting it.
    #[test]
    fn usage_counts_reflects_shared_and_unused_tunnels() {
        let store = TunnelStore::new(temp_config_path());
        store
            .create("qa-bastion", sample_tunnel("QA"), None)
            .unwrap();
        store
            .create("unused-bastion", sample_tunnel("Unused"), None)
            .unwrap();
        store.bind("qa-1", "qa-bastion").unwrap();
        store.bind("qa-2", "qa-bastion").unwrap();

        let counts = store.usage_counts();

        assert_eq!(counts.get("qa-bastion"), Some(&2));
        assert_eq!(counts.get("unused-bastion"), Some(&0));
    }

    /// Tasks.md 1.2: a binding whose context is absent from the given list is stale.
    #[test]
    fn stale_bindings_lists_contexts_absent_from_the_given_list() {
        let store = TunnelStore::new(temp_config_path());
        store
            .create("qa-bastion", sample_tunnel("QA"), None)
            .unwrap();
        store.bind("qa-1", "qa-bastion").unwrap();
        store.bind("renamed-away", "qa-bastion").unwrap();

        let stale = store.stale_bindings(&["qa-1".to_string()]);

        assert_eq!(
            stale,
            vec![("renamed-away".to_string(), "qa-bastion".to_string())]
        );
    }
}
