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
    // The causes are shown through `Debug` (the editor's and connect path's messages),
    // which dead-code analysis doesn't count as a read.
    #[allow(dead_code)]
    Secret(keyring::Error),
    #[allow(dead_code)]
    Io(io::Error),
}

/// A single invalid field on a [`TunnelConfig`] passed to
/// [`TunnelStore::create`]/[`TunnelStore::update`]. Ports are `u16`, so the only
/// out-of-range value in 1-65535 is `0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelFieldError {
    EmptyHost,
    EmptyUser,
    InvalidPort,
    /// A jump host's user or host starts with `-`, which ssh could read as an option.
    OptionLikeJumpHost,
    /// A command tunnel's command line is empty.
    EmptyCommand,
    /// A command tunnel's command line has an unclosed quote.
    UnbalancedQuotes,
    /// A command tunnel's command has no `{port}` and no fixed local port, so there is
    /// no way to know which port it listens on.
    NoPortPlaceholder,
    /// A command tunnel's fixed local port is outside 1-65535.
    InvalidLocalPort,
    /// A command tunnel's startup timeout is not a positive number of seconds.
    InvalidTimeout,
}

/// Collects every violation of the validation rules for `tunnel`'s kind - for SSH,
/// tasks.md 1.2's (non-empty host and user, port 1-65535); for a command tunnel,
/// `command-tunnels` 1.3's. Only the active kind's fields are checked: the other
/// kind's are kept but unused. Empty, not `None`, when `tunnel` is valid.
pub(super) fn validate(tunnel: &TunnelConfig) -> Vec<TunnelFieldError> {
    match tunnel.kind {
        TunnelKind::Ssh => validate_ssh(tunnel),
        TunnelKind::Command => validate_command(&tunnel.command),
        // Every manual setting is valid: an empty message shows only the name.
        TunnelKind::Manual => Vec::new(),
    }
}

fn validate_command(command: &CommandTunnelConfig) -> Vec<TunnelFieldError> {
    use crate::tunnel::command::argv::{self, ArgvError};
    let mut errors = Vec::new();
    match argv::split(&command.command_line) {
        Err(ArgvError::Empty) => errors.push(TunnelFieldError::EmptyCommand),
        Err(ArgvError::UnbalancedQuotes) => errors.push(TunnelFieldError::UnbalancedQuotes),
        Ok(args) if !argv::has_port_placeholder(&args) && command.local_port.is_none() => {
            errors.push(TunnelFieldError::NoPortPlaceholder);
        }
        Ok(_) => {}
    }
    if command.local_port == Some(0) {
        errors.push(TunnelFieldError::InvalidLocalPort);
    }
    if command.startup_timeout_secs == 0 {
        errors.push(TunnelFieldError::InvalidTimeout);
    }
    errors
}

fn validate_ssh(tunnel: &TunnelConfig) -> Vec<TunnelFieldError> {
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
    if tunnel
        .jump_hosts
        .iter()
        .any(|hop| crate::tunnel::ssh::is_option_like_jump_host(hop))
    {
        errors.push(TunnelFieldError::OptionLikeJumpHost);
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

#[cfg(test)]
mod tests;
