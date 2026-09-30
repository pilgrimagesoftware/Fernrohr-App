//! Owns `context -> tunnel id` binding: reading a context's binding (or all of
//! them), creating/overwriting one, removing one, and the read-only queries built on
//! top - per-tunnel usage counts for the section 4 Tunnels panel, and bindings left
//! stale by a renamed/removed kubeconfig context.

use super::*;

impl TunnelStore {
    /// The tunnel id `context` is bound to, if any - section 6.2's connect path uses
    /// this to decide whether to route a context's connection through a forward.
    pub fn binding_for(&self, context: &str) -> Option<String> {
        let config: TunnelsConfig = config::load(&self.config_path);
        config.context_bindings.get(context).cloned()
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
mod tests;
