use super::connection::{ClusterConnection, ConnectionState};
use super::context_health::ContextHealth;
use super::health::{self, HealthTransition};
use super::watch_registry::{PauseReason, WatchRegistry};
use crate::k8s::resource::pods::{PodsTable, watch_all_namespaces};
use gpui_kit::{App, AppContext as _, Entity, Global, WindowId};
use kube::Client;
use std::collections::{HashMap, HashSet};

mod holds;
mod pause_resume;
mod pods_watch;
mod registry;
#[cfg(test)]
mod test_support;

pub use registry::ClusterRegistry;
