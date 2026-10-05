//! Section 4.1 of the `tunnel-management-ui` change: the Tunnels window shell - a
//! single-instance OS window listing every tunnel (name, host, read-only usage count,
//! running state from section 2.2's watched `ForwardKey` set) and a stale-bindings
//! section with a per-row Remove. A "New Tunnel" control and each row's Edit open
//! `editor::TunnelEditor` in the pane below; the window itself never assigns a
//! context to a tunnel (proposal.md, design.md decision 5) - that lives on the
//! context side (`ui/picker_tunnel.rs`, `util/shell.rs`'s `context.set_tunnel`).

use super::editor::{TunnelEditor, TunnelEditorEvent};
use super::{TunnelsRevision, notify_tunnels_changed};
use crate::command::{Command, CommandRegistry};
use crate::config::tunnels::{CommandTunnelMode, TunnelConfig, TunnelKind};
use crate::consts::{TUNNELS_WINDOW_MIN_SIZE, TUNNELS_WINDOW_SIZE};
use crate::k8s::cluster::kubeconfig;
use crate::k8s::cluster::tunnel::{self, ForwardKey};
use crate::tunnel::store::TunnelStore;
use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Root, Sizable as _};
use gpui_kit::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

mod command;
pub(super) mod port_forwards;
mod render;
mod window;

use window::*;

pub use command::{TunnelsManage, register_commands};
pub use window::open_or_focus;
