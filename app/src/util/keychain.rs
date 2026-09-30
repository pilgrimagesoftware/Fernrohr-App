//! Spike for section 1.1 of the tunnel-subsystem change: confirms `keyring` reaches the
//! macOS Keychain and, where available, the Linux Secret Service, and that a missing backend
//! surfaces as a catchable error rather than a panic. The production wrapper (service = app
//! id, account = tunnel id, fallback prompt) lands in section 5.2.

use keyring::Entry;

const SMOKE_SERVICE: &str = "com.pilgrimagesoftware.fernrohr.smoke-test";

/// Writes, reads back, and deletes a secret for `account`. `Ok(true)` means the round trip
/// matched; `Ok(false)` means no credential backend is available (caller should fall back);
/// any other failure is returned as `Err`.
// UNWIRED(#3): section 1.1 spike only. Section 5.2 wires the production keychain wrapper
// (service = app id, account = tunnel id, in-memory prompt fallback) around this same crate.
#[allow(dead_code)]
pub fn smoke_test(account: &str, secret: &str) -> keyring::Result<bool> {
    let entry = match Entry::new(SMOKE_SERVICE, account) {
        Ok(entry) => entry,
        Err(keyring::Error::NoDefaultStore) => return Ok(false),
        Err(err) => return Err(err),
    };

    match entry.set_password(secret) {
        Ok(()) => {}
        Err(keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_)) => {
            return Ok(false);
        }
        Err(err) => return Err(err),
    }

    // Delete unconditionally, even if the read-back fails: a fixed `account`
    // means a credential the read step errors out on (before reaching the
    // delete this function used to only reach on success) survives the
    // process and pollutes every later run against the same account.
    let read_back = entry.get_password();
    let _ = entry.delete_credential();
    Ok(read_back? == secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique per process *and* per run: a run interrupted before `smoke_test`'s own
    /// cleanup (or this test's guard) runs must not leave behind an account a later
    /// run could collide with.
    fn unique_account() -> String {
        let pid = std::process::id();
        let nanos = jiff::Timestamp::now().as_nanosecond();
        format!("keychain-smoke-test-account-{pid}-{nanos}")
    }

    /// Deletes `account`'s entry on drop, even if the test panics partway through -
    /// covers the case where `smoke_test` itself already stored the secret but a
    /// later step (its own read-back, or this test's assertion) fails before its
    /// internal `delete_credential` call runs.
    struct CleanupGuard {
        account: String,
    }

    impl Drop for CleanupGuard {
        fn drop(&mut self) {
            if let Ok(entry) = Entry::new(SMOKE_SERVICE, &self.account) {
                let _ = entry.delete_credential();
            }
        }
    }

    #[test]
    #[ignore = "touches the real OS keychain; run with `cargo test -- --ignored`"]
    fn round_trips_or_reports_no_backend() {
        let account = unique_account();
        let _cleanup = CleanupGuard {
            account: account.clone(),
        };

        match smoke_test(&account, "s3cr3t-value") {
            Ok(matched) => assert!(matched, "stored secret did not match on read-back"),
            Err(err) => panic!("keyring backend present but operation failed: {err:?}"),
        }
    }
}
