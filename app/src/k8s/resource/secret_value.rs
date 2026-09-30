//! One Secret value, revealed on request - and the only way the application
//! ever holds one (`pod-configuration-tab`'s "hidden until revealed").
//!
//! A [`SecretValue`] is made by [`reveal`]: a fresh `get` of one Secret, from
//! which exactly one key's decoded bytes are kept and the rest of the response
//! dropped, so an unrevealed value never exists in the process at all. The type
//! makes the other rules structural rather than a convention:
//!
//! - its `Debug` and `Display` print `<secret: N bytes>`, never the value, so a
//!   value can't reach a log line through a derived `Debug` on a containing type;
//! - it has no `Serialize`, so it can't end up in a saved layout or config file;
//! - the bytes come out only through [`SecretValue::expose`], so every place a
//!   value is read shows up in `git grep 'expose('`.

use k8s_openapi::api::core::v1::Secret;
use kube::Api;
use std::fmt;

/// One Secret key's decoded value.
pub struct SecretValue(Vec<u8>);

impl SecretValue {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// The value's size in bytes - what every view shows in place of it.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the value is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The value as text, for the one element that draws a revealed value.
    /// `None` for a value that isn't UTF-8, which is shown as "binary, N bytes"
    /// rather than rendered.
    pub fn expose(&self) -> Option<&str> {
        std::str::from_utf8(&self.0).ok()
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<secret: {} bytes>", self.len())
    }
}

impl fmt::Display for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// Why a reveal produced no value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevealError {
    /// The Secret, or the key in it, doesn't exist (any more).
    Missing,
    /// The read failed - the API's own message, which never contains a value.
    Failed(String),
}

/// One Secret key's reveal, while it's pending or shown - the state both the
/// Configuration tab and the object viewer keep per revealed key.
#[derive(Debug)]
pub enum Reveal {
    Pending,
    Shown(SecretValue),
    Failed(RevealError),
}

/// Reads Secret `namespace/name` afresh and keeps only `key`'s value.
///
/// Everything else in the response - the other keys' values, the manifest's
/// annotations - is dropped before this returns.
pub async fn reveal(
    client: kube::Client,
    namespace: String,
    name: String,
    key: String,
) -> Result<SecretValue, RevealError> {
    let api: Api<Secret> = Api::namespaced(client, &namespace);
    let secret = match api.get(&name).await {
        Ok(secret) => secret,
        Err(kube::Error::Api(status)) if status.code == 404 => return Err(RevealError::Missing),
        Err(error) => return Err(RevealError::Failed(crate::k8s::error::describe(&error))),
    };
    let mut data = secret.data.unwrap_or_default();
    data.remove(&key)
        .map(|bytes| SecretValue::new(bytes.0))
        .ok_or(RevealError::Missing)
}

#[cfg(test)]
mod tests;
