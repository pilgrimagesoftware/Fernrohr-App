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
//!
//! `errors.rs` owns the error types and validation, `crud.rs` owns `TunnelStore`
//! itself and its create/read/update/delete surface, and `bindings.rs` owns the
//! `context -> tunnel id` binding surface built on top.
// UNWIRED(#3): section 6.2's connect-path integration is the first real caller.
#![allow(dead_code)]

use crate::config::{
    self,
    tunnels::{CommandTunnelConfig, TunnelConfig, TunnelKind, TunnelsConfig},
};
use crate::tunnel::secrets::TunnelSecretStore;
use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

mod bindings;
mod crud;
mod errors;
#[cfg(test)]
mod test_support;

pub use crud::TunnelStore;
pub use errors::{TunnelFieldError, TunnelStoreError};

use errors::*;
