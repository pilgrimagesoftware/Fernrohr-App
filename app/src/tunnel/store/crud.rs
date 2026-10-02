//! Owns `TunnelStore`'s definition and its CRUD surface: construction (with the
//! test-only in-memory secret store swapped in under `cfg(test)`), reading a tunnel's
//! config or stored secret, and create/update/delete, each keeping `tunnels.toml` and
//! the keychain in sync in one call. `delete` also unbinds every context that pointed
//! at the deleted tunnel - see `bindings.rs` for the `context -> tunnel id` side.

use super::*;

pub struct TunnelStore {
    pub(super) config_path: PathBuf,
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
}

#[cfg(test)]
mod tests;
