//! Resource-kind navigation: switches which resource kind a connected window's
//! active panel displays (Pods or Logs), without dropping that window's
//! `ClusterSession` connection. See the `cluster-picker-and-navigation`
//! change's `resource-browser` spec delta.

use crate::command::{Command, CommandRegistry};
use gpui_kit::assets::IconName;
use gpui_kit::component::dock::{DockLayout, panel_handle};
use gpui_kit::*;

actions!(nav, [ShowPods, ShowLogs]);

pub const SHOW_PODS_COMMAND_ID: &str = "nav.show_pods";
pub const SHOW_PODS_DEFAULT_BINDING: &str = "cmd-1";
pub const SHOW_LOGS_COMMAND_ID: &str = "nav.show_logs";
pub const SHOW_LOGS_DEFAULT_BINDING: &str = "cmd-2";

/// A resource kind a connected window's active panel can display. Deliberately a
/// small, explicit enum rather than a discovery-driven list - see the change's
/// design doc "Navigation is a fixed sidebar" decision.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NavTarget {
    Pods,
    Logs,
}

impl NavTarget {
    pub fn label(self) -> &'static str {
        match self {
            NavTarget::Pods => "Pods",
            NavTarget::Logs => "Logs",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            NavTarget::Pods => IconName::Boxes,
            NavTarget::Logs => IconName::ScrollText,
        }
    }
}

/// The commands this module contributes to the app-wide [`CommandRegistry`],
/// so the command palette and the sidebar's click targets dispatch the same
/// actions - see `shell::register_commands` for the sibling pattern.
pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: SHOW_PODS_COMMAND_ID,
        title: "Show Pods",
        default_binding: SHOW_PODS_DEFAULT_BINDING,
        context: None,
        action: Box::new(ShowPods),
    });
    registry.register(Command {
        id: SHOW_LOGS_COMMAND_ID,
        title: "Show Logs",
        default_binding: SHOW_LOGS_DEFAULT_BINDING,
        context: None,
        action: Box::new(ShowLogs),
    });
}

/// Builds a single-panel `DockLayout` for `target`, backed by `context_name`'s
/// `ClusterSession` - the same session `dock_area`'s previous panel used, so
/// switching kind never reconnects.
pub fn build_layout(target: NavTarget, context_name: String, cx: &mut App) -> DockLayout {
    match target {
        NavTarget::Pods => {
            let panel = cx.new(|cx| crate::pods::PodsPanel::new(context_name, cx));
            DockLayout::tabs().panel_view(panel_handle(panel), cx)
        }
        NavTarget::Logs => {
            let panel = cx.new(|cx| crate::logs::LogsPanel::new(context_name, cx));
            DockLayout::tabs().panel_view(panel_handle(panel), cx)
        }
    }
}
