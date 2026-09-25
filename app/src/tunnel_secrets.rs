//! Section 5.2 of the tunnel-subsystem change: the production keychain wrapper around
//! section 1.1's `keyring` spike.
//!
//! Secrets are keyed `(service = TUNNEL_KEYCHAIN_SERVICE, account = tunnel id)`. When no
//! credential backend is available - the same `NoDefaultStore`/`NoStorageAccess`/
//! `PlatformFailure` cases the 1.1 smoke test already classifies as "no backend" rather
//! than a hard failure - every operation falls back to an in-memory map guarded by a
//! `parking_lot::Mutex`. That map is per-process: it does not survive a restart, so a
//! machine with no keychain re-prompts for tunnel secrets each session rather than
//! silently persisting them to disk.
// UNWIRED(#3): section 5.3's tunnel CRUD UI is the first real caller.
#![allow(dead_code)]

use crate::consts::TUNNEL_KEYCHAIN_SERVICE;
use keyring::Entry;
use parking_lot::Mutex;
use std::collections::HashMap;

/// True for the keyring errors that mean "no backend available", not "this specific
/// operation failed" - the same set section 1.1's smoke test already treats as a
/// fallback signal rather than a hard error.
fn is_backend_unavailable(err: &keyring::Error) -> bool {
    matches!(
        err,
        keyring::Error::NoDefaultStore
            | keyring::Error::NoStorageAccess(_)
            | keyring::Error::PlatformFailure(_)
    )
}

pub struct TunnelSecretStore {
    fallback: Mutex<HashMap<String, String>>,
}

impl Default for TunnelSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TunnelSecretStore {
    pub fn new() -> Self {
        Self {
            fallback: Mutex::new(HashMap::new()),
        }
    }

    fn entry(tunnel_id: &str) -> keyring::Result<Entry> {
        Entry::new(TUNNEL_KEYCHAIN_SERVICE, tunnel_id)
    }

    /// Stores `secret` for `tunnel_id`, in the keychain if available, else the
    /// in-memory fallback.
    pub fn save(&self, tunnel_id: &str, secret: &str) -> keyring::Result<()> {
        let entry = match Self::entry(tunnel_id) {
            Ok(entry) => entry,
            Err(err) if is_backend_unavailable(&err) => {
                self.fallback
                    .lock()
                    .insert(tunnel_id.to_string(), secret.to_string());
                return Ok(());
            }
            Err(err) => return Err(err),
        };

        match entry.set_password(secret) {
            Ok(()) => {
                // A prior fallback save must not shadow the now-available keychain entry.
                self.fallback.lock().remove(tunnel_id);
                Ok(())
            }
            Err(err) if is_backend_unavailable(&err) => {
                self.fallback
                    .lock()
                    .insert(tunnel_id.to_string(), secret.to_string());
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    /// Reads the secret for `tunnel_id`. `Ok(None)` means no secret is stored (neither
    /// backend has one); this is not an error.
    pub fn read(&self, tunnel_id: &str) -> keyring::Result<Option<String>> {
        let entry = match Self::entry(tunnel_id) {
            Ok(entry) => entry,
            Err(err) if is_backend_unavailable(&err) => {
                return Ok(self.fallback.lock().get(tunnel_id).cloned());
            }
            Err(err) => return Err(err),
        };

        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(self.fallback.lock().get(tunnel_id).cloned()),
            Err(err) if is_backend_unavailable(&err) => {
                Ok(self.fallback.lock().get(tunnel_id).cloned())
            }
            Err(err) => Err(err),
        }
    }

    /// Deletes the secret for `tunnel_id` from whichever backend holds it. Deleting an
    /// absent secret is not an error.
    pub fn delete(&self, tunnel_id: &str) -> keyring::Result<()> {
        self.fallback.lock().remove(tunnel_id);

        let entry = match Self::entry(tunnel_id) {
            Ok(entry) => entry,
            Err(err) if is_backend_unavailable(&err) => return Ok(()),
            Err(err) => return Err(err),
        };

        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) if is_backend_unavailable(&err) => Ok(()),
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_tunnel_id(name: &str) -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("tunnel-secrets-test-{name}-{n}")
    }

    #[test]
    fn save_read_delete_round_trip() {
        let store = TunnelSecretStore::new();
        let id = unique_tunnel_id("round-trip");

        assert_eq!(store.read(&id).unwrap(), None);

        store.save(&id, "s3cr3t").unwrap();
        assert_eq!(store.read(&id).unwrap(), Some("s3cr3t".to_string()));

        store.delete(&id).unwrap();
        assert_eq!(store.read(&id).unwrap(), None);
    }

    #[test]
    fn delete_of_absent_secret_is_not_an_error() {
        let store = TunnelSecretStore::new();
        let id = unique_tunnel_id("delete-absent");
        store.delete(&id).unwrap();
    }

    /// Exercises the in-memory fallback path directly, independent of whether this
    /// machine actually has a working keychain backend: `is_backend_unavailable`'s
    /// three variants are exactly what `Entry::new`/`set_password`/`get_password`
    /// return when no backend is present, so driving the fallback map through the same
    /// public API proves the fallback logic without needing to fake `keyring` itself.
    #[test]
    fn fallback_map_round_trips_independent_of_keychain() {
        let store = TunnelSecretStore::new();
        let id = unique_tunnel_id("fallback");

        store
            .fallback
            .lock()
            .insert(id.clone(), "from-memory".into());
        assert_eq!(store.read(&id).unwrap(), Some("from-memory".to_string()));

        store.delete(&id).unwrap();
        assert!(!store.fallback.lock().contains_key(&id));
    }
}
