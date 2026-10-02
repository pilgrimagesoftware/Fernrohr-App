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
// UNWIRED(#3): `tunnel_store::TunnelStore` (section 5.3) is the first real caller;
// section 6's context binding/connect-path work is the first caller of `TunnelStore`
// itself, at which point this module is reachable outside tests.
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
    /// When `true`, every operation below takes the same in-memory fallback path
    /// production code already takes when no keychain backend is available, and never
    /// constructs a real `keyring::Entry` at all. Only reachable via
    /// [`Self::new_in_memory_only`], so [`Self::new`] (and so production behavior) is
    /// unaffected.
    force_fallback_only: bool,
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
            force_fallback_only: false,
        }
    }

    /// Test-only: a store that never touches the OS keychain, so tests that only care
    /// about `TunnelStore`'s book-keeping (not real keychain integration) never trigger
    /// a macOS Keychain access prompt. `TunnelStore::new` uses this under `cfg(test)`,
    /// so every `TunnelStore` built in a test binary - directly by its own tests, or
    /// indirectly through the tunnel editor and connect-path code under test - gets it.
    #[cfg(test)]
    pub(crate) fn new_in_memory_only() -> Self {
        Self {
            fallback: Mutex::new(HashMap::new()),
            force_fallback_only: true,
        }
    }

    fn entry(tunnel_id: &str) -> keyring::Result<Entry> {
        Entry::new(TUNNEL_KEYCHAIN_SERVICE, tunnel_id)
    }

    /// Stores `secret` for `tunnel_id`, in the keychain if available, else the
    /// in-memory fallback.
    pub fn save(&self, tunnel_id: &str, secret: &str) -> keyring::Result<()> {
        if self.force_fallback_only {
            self.fallback
                .lock()
                .insert(tunnel_id.to_string(), secret.to_string());
            return Ok(());
        }

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
        if self.force_fallback_only {
            return Ok(self.fallback.lock().get(tunnel_id).cloned());
        }

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

        if self.force_fallback_only {
            return Ok(());
        }

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

    /// Unique per process *and* per run, not just per counter: a run interrupted
    /// before cleanup (a killed `cargo test`, a crashed harness) must not leave behind
    /// an id a *later* run could collide with and mistake for its own leftover entry.
    fn unique_tunnel_id(name: &str) -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let nanos = jiff::Timestamp::now().as_nanosecond();
        format!("tunnel-secrets-test-{name}-{pid}-{nanos}-{n}")
    }

    /// Deletes `id` from `store` on drop, even if a `#[test]` fn panics partway
    /// through - so an assertion failure on the real keychain path still leaves the
    /// backend clean for the next run rather than a stray entry under this id.
    struct CleanupGuard<'a> {
        store: &'a TunnelSecretStore,
        id: String,
    }

    impl Drop for CleanupGuard<'_> {
        fn drop(&mut self) {
            let _ = self.store.delete(&self.id);
        }
    }

    /// The one test in this module that exercises the real OS keychain (or, on a
    /// machine with no backend, the same fallback every other test here uses
    /// in-memory-only): ignored so a plain `cargo test` never triggers a macOS
    /// Keychain access prompt.
    #[test]
    #[ignore = "touches the real OS keychain; run with `cargo test -- --ignored`"]
    fn save_read_delete_round_trip() {
        let store = TunnelSecretStore::new();
        let id = unique_tunnel_id("round-trip");
        let _cleanup = CleanupGuard {
            store: &store,
            id: id.clone(),
        };

        assert_eq!(store.read(&id).unwrap(), None);

        store.save(&id, "s3cr3t").unwrap();
        assert_eq!(store.read(&id).unwrap(), Some("s3cr3t".to_string()));

        store.delete(&id).unwrap();
        assert_eq!(store.read(&id).unwrap(), None);
    }

    #[test]
    fn delete_of_absent_secret_is_not_an_error() {
        let store = TunnelSecretStore::new_in_memory_only();
        let id = unique_tunnel_id("delete-absent");
        store.delete(&id).unwrap();
    }

    /// Exercises the in-memory fallback path directly: `new_in_memory_only` proves
    /// the fallback logic itself through the same public API `save`/`read`/`delete`
    /// use, independent of whether this machine has a working keychain backend at all.
    #[test]
    fn fallback_map_round_trips_independent_of_keychain() {
        let store = TunnelSecretStore::new_in_memory_only();
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
