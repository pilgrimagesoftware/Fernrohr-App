//! The Pods list: a live, filterable, sortable table of every pod a context
//! can see. This file declares and re-exports; each submodule owns one concern
//! (see their own doc comments). The imports below are the submodules' shared
//! vocabulary - each opens with `use super::*`.

use crate::config::workspace::{NamespaceScope, SortState};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::{self, PanelScope, ScopeEvent};
use crate::util::resource_index::ResourceIndex;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::dock::{
    BasePanel, Panel, PanelControl, PanelEvent, PanelInfo, PanelState, panel_handle, register_panel,
};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::table::{TableEvent, TableState};
use gpui_kit::*;
use jiff::Timestamp;
use k8s_openapi::api::core::v1::Pod;
use kube_runtime::watcher;

use super::pods_table::{self, PodTableDelegate, PodTableRow};
mod actions;
mod commands;
mod hints;
#[cfg(test)]
pub(crate) use commands::LIST_KEY_CONTEXT;
pub use commands::{
    CloseQuickLook, DeletePod, DescribePod, KillPod, OpenQuickLookDetails, PANEL_KEY_CONTEXT,
    PortForwardPod, QUICK_LOOK_KEY_CONTEXT, QuickLook, ShellPod, ShowPodLogs, ShowPodYaml,
    WarpAllToNamespace, WarpNamespace, register_commands,
};
use commands::{DESCRIBE_KEY, LOGS_KEY, NAMESPACE_KEY, QUICK_LOOK_KEY, WARP_ALL_KEY, YAML_KEY};

mod panel;
mod port_forward;
pub(super) mod quick_look;
#[cfg(test)]
mod real_window_tests;
mod render;
mod rows;
mod selection;
mod shell;
mod store;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod test_window;
mod watch;

pub use panel::{PodsPanel, register_restore};
pub(crate) use rows::BAD_WAITING_REASONS;
pub(crate) use rows::format_age;
pub use rows::{PodRow, matches_namespaces, pod_row};
pub use selection::{PodSelection, SelectedPod};
pub use shell::SHELL_KEY_CONTEXT;
pub use store::PodsTable;
pub use watch::watch_all_namespaces;

// Everything else, for the submodules' `use super::*` and the tests.
use rows::*;
use selection::*;
