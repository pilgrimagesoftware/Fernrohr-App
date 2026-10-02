//! Per-context cluster connection: state (`state.rs`), the connect path
//! (`connect.rs` - tunnel wait, config resolution, rewrite) and probing
//! (`probe.rs`). This file declares and re-exports; each submodule owns one
//! concern (see their own doc comments). The imports below are the
//! submodules' shared vocabulary - each opens with `use super::*`.

use super::tunnel::{self, ForwardKey};
use crate::forward::managed::{ForwardState, ManagedForward as _};
use crate::forward::registry::RegistryHandle;
use crate::tunnel::ssh::SshTunnel;
use gpui_kit::{App, AppContext as _, Context, Entity};
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Client, Config};
use std::net::SocketAddr;
use std::time::Instant;
use tokio::sync::{mpsc, watch};

mod connect;
mod probe;
mod state;
#[cfg(test)]
mod test_support;

pub(in crate::k8s::cluster) use connect::{connect_and_probe, resolve_config};
pub(crate) use probe::error_chain;
pub use probe::probe;
pub use state::{ClusterConnection, ConnectionState};
