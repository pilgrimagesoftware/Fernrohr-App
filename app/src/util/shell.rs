//! The application shell: windows, their workspaces, and the commands and
//! persistence around them. This file declares and re-exports; each submodule
//! owns one concern (see their own doc comments).
//!
//! The imports below are the submodules' shared vocabulary: each opens with
//! `use super::*`, reaching both these and its siblings (through the private
//! globs further down). The test files use named imports instead - see the
//! note at the top of any of them.

use crate::command::{Command, CommandRegistry};
use crate::config::{
    self,
    workspace::{PanelDescriptor, WindowLayout, WorkspaceConfig},
};
use crate::consts::{RESOURCE_PANEL_MAX_WIDTH, RESOURCE_PANEL_MIN_WIDTH, RESOURCE_PANEL_WIDTH};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pod_detail::DetailView;
use crate::k8s::resource::pods::SelectedPod;
use crate::keymap;
use crate::tunnel::store::TunnelStore;
use crate::ui::nav::{
    self, NavTarget, OpenMode, OpenedPanel, ShowLogs, ShowPodDetail, ShowPodDetailYaml, ShowPods,
};
use crate::ui::panel_title::{self, PanelScope};
use crate::ui::picker_tunnel;
use crate::ui::tunnels;
use crate::util::context_lifecycle;
use crate::util::paths;
use gpui_kit::component::Root;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dock::{
    DockArea, DockEvent, DockPlacement, DockSkin, PanelId, PanelInfo, PanelState,
};
use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

mod actions;
mod app;
mod arrange;
mod contexts;
mod empty_dock;
mod exec;
mod follow;
mod layout;
mod logs_open;
mod main_window;
mod namespace_defaults;
mod namespace_sets;
mod warp;
pub(crate) use exec::OpenExecSession;
pub(crate) use warp::WarpContextToNamespace;
#[cfg(test)]
mod chord_precedence_tests;
#[cfg(test)]
mod edit_yaml_tests;
#[cfg(test)]
mod label_logs_tests;
#[cfg(test)]
mod node_pods_window_tests;
mod open;
mod panel_focus;
mod panels;
#[cfg(test)]
mod pending_chord_tests;
mod persist;
#[cfg(test)]
mod pod_detail_logs_tests;
#[cfg(test)]
mod quick_look_window_tests;
mod render;
mod resource_edge;
mod saved_layouts;
mod tab_focus;
mod tabs;
mod test_hooks;
#[cfg(test)]
mod test_support;
mod tunnel_dialog;
mod window;
mod window_title;

// The surface the rest of the crate uses, named explicitly: a glob would
// collide with the `gpui_kit::*` import above (`gpui_kit::init` versus
// `app::init`, `gpui_kit::open_window` versus `window::open_window`). Named
// here, these shadow both globs, so `use super::*` in a submodule sees ours.
#[cfg(test)]
pub(crate) use app::register_commands;
pub use app::{SetContextTunnel, default_workspace_path, init};
pub(crate) use app::{TOGGLE_PALETTE_DEFAULT_BINDING, ToggleCommandPalette};
pub(crate) use layout::{close_window, window_context_count};
pub use main_window::MainWindow;
pub use persist::open_saved_or_default;
// `ui::settings::layouts` (`saved-panel-layouts` tasks 6.1) reads saved
// layouts to list them, so it needs the same directory `layouts.save`/
// `layouts.manage` already read and write. `layouts_dir` itself is
// `pub(crate)` (see its own doc comment) just so this one re-export is
// legal; `SavedLayoutsDir` - the test-override global it reads - and the
// `saved_layouts` module stay unwidened.
pub(crate) use saved_layouts::{SavedLayoutsChanged, layouts_dir, note_layouts_changed};
pub(crate) use tabs::close_panel;
pub(crate) use window::open_window;

// Everything else, for the submodules' `use super::*` and the tests: each
// submodule reaches its siblings through this module.
use app::*;
use empty_dock::*;
use layout::*;
use panels::*;
use persist::*;
use tunnel_dialog::*;
use window::*;

/// Test-only: points [`layouts_dir`] at `dir` instead of the real
/// `state_dir()/layouts/` - the same [`SavedLayoutsDir`] override
/// `util::shell`'s own saved-layout tests already set directly, exposed here
/// (rather than widening `SavedLayoutsDir` itself, a tuple struct whose field
/// is `pub(super)`) so `ui::settings`'s Layouts section can be tested against
/// a temp directory too.
#[cfg(test)]
pub(crate) fn set_layouts_dir_for_test(dir: PathBuf, cx: &mut App) {
    cx.set_global(SavedLayoutsDir(dir));
}
