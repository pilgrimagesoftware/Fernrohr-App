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

    let read_back = entry.get_password()?;
    entry.delete_credential()?;
    Ok(read_back == secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_or_reports_no_backend() {
        match smoke_test("keychain-smoke-test-account", "s3cr3t-value") {
            Ok(matched) => assert!(matched, "stored secret did not match on read-back"),
            Err(err) => panic!("keyring backend present but operation failed: {err:?}"),
        }
    }
}
