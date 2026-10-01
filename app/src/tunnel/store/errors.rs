//! Owns the tunnel store's error types: [`TunnelStoreError`] (the result of every
//! CRUD/bind/unbind call) and [`TunnelFieldError`] (one entry per invalid field a
//! `create`/`update` call rejected), plus the validation and `From` conversions around
//! them.

use super::*;

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
pub(super) fn validate(tunnel: &TunnelConfig) -> Vec<TunnelFieldError> {
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
