//! The cluster picker: shown in place of a window's panel workspace whenever that window
//! has no open or restored panels. Lists kubeconfig contexts through the same searchable
//! `Command` widget the app-wide command palette uses, drives a connection through
//! `ClusterRegistry`, and emits [`PickerEvent::Connected`] on success so `shell::MainWindow`
//! can switch that window into its normal panel workspace.
//!
//! A context row no longer connects on a single click: `Command`'s own row wrapper
//! confirms on any click, which made an inadvertent click connect. `context_row`
//! intercepts the click first (see its doc comment) and routes it through
//! [`ClusterPicker::handle_row_click`] instead - a single click only moves the
//! highlight, a double click connects. [`ClusterPicker::connect_button`] and `Enter`
//! (via [`ClusterPicker::confirm_row`]) are the deliberate ways to connect the
//! highlighted row.
//!
//! Split by concern, each an inherent `impl ClusterPicker` slice or a group of free
//! functions: [`state`] owns construction, persisted state, and the connect/tunnel
//! plumbing; [`interaction`] owns click/keyboard selection; [`layout`] owns the
//! picker's sizing constants and static chrome; [`rows`] builds the per-row and
//! control elements; [`render`] wires all of the above into `Render for ClusterPicker`.
//! This file stays the module root: shared imports, submodule declarations, and the
//! re-exports the rest of the crate reaches through `ui::picker::`.

use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::kubeconfig;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::tunnel::store::TunnelStore;
use crate::ui::picker_tunnel::{self, TunnelChoice};
use crate::ui::tunnels::TunnelsRevision;
use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::command::{Command, CommandItem, CommandState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, IndexPath, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::BTreeMap;
use std::path::PathBuf;

mod interaction;
mod layout;
mod logo;
mod render;
mod rows;
pub mod saved_layouts;
mod state;

use layout::*;
use rows::*;

pub use layout::MIN_WINDOW_SIZE;
pub(crate) use state::load_tunnels;
pub use state::{ClusterPicker, PickerEvent};
