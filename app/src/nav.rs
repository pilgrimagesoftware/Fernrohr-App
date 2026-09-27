//! Which panel a selected resource kind opens. Section 8.2 of the
//! `cluster-picker-and-navigation` change: a kind with a concrete panel gets
//! it, and every other discovered kind gets a placeholder, so the Resource
//! panel never offers a row that opens nothing.

use crate::cluster::discovery::DiscoveredKind;
use crate::command::{Command, CommandRegistry};
use gpui_kit::assets::IconName;
use gpui_kit::component::dock::{DockArea, DockPlacement, PanelId};
use gpui_kit::*;

actions!(nav, [ShowPods, ShowLogs]);

pub const SHOW_PODS_COMMAND_ID: &str = "nav.show_pods";
pub const SHOW_PODS_DEFAULT_BINDING: &str = "cmd-1";
pub const SHOW_LOGS_COMMAND_ID: &str = "nav.show_logs";
pub const SHOW_LOGS_DEFAULT_BINDING: &str = "cmd-2";

/// What a connected window's active panel is showing. Which options exist is
/// no longer a fixed set here - the Resource panel's discovery-driven list
/// decides that - so a target is either one discovered kind, or the log view.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum NavTarget {
    /// A kind the cluster's API discovery reported.
    Kind(DiscoveredKind),
    /// The container-log view over the selected pod. Not a discovered kind but
    /// a view onto one, so it stays a target of its own rather than being
    /// forced into the `Kind` shape.
    Logs,
}

impl NavTarget {
    /// The target of the `nav.show_pods` command. Built without discovery, so
    /// it is pinned to the core `v1` `Pod` kind every cluster reports.
    pub fn pods() -> Self {
        NavTarget::Kind(DiscoveredKind::pods())
    }

    pub fn label(&self) -> String {
        match self {
            NavTarget::Kind(kind) => kind.label(),
            NavTarget::Logs => "Logs".to_string(),
        }
    }

    pub fn icon(&self) -> IconName {
        match self {
            NavTarget::Kind(_) => IconName::Box,
            NavTarget::Logs => IconName::ScrollText,
        }
    }
}

/// Whether this build has a concrete panel for `kind`. Pods is the only one
/// today; everything else falls through to
/// [`crate::placeholder::PlaceholderPanel`].
pub fn has_concrete_panel(kind: &DiscoveredKind) -> bool {
    kind.gvk.group.is_empty() && kind.gvk.kind == "Pod"
}

/// The commands this module contributes to the app-wide [`CommandRegistry`],
/// so the command palette and the Resource panel's rows dispatch the same
/// actions - see `shell::register_commands` for the sibling pattern.
///
/// Only the two panel-opening actions that exist without discovery can be
/// registered here. A per-kind command would have to be minted at runtime from
/// the cluster's kinds, and the palette is built from the registry at app
/// start - before any cluster is connected.
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

/// Builds the panel for `target` in `context_name`'s session and adds it to
/// `area`'s centre, returning the dock's id for it.
///
/// One code path for "open a panel", so a kind opened from the Resource panel,
/// from the context menu, or from `nav.show_pods` cannot drift apart. The
/// caller owns deduplication: whether this is a new panel or a focus of an
/// existing one is the window's bookkeeping, not the panel's.
pub fn add_panel(
    area: &mut DockArea,
    target: &NavTarget,
    context_name: String,
    window: &mut Window,
    cx: &mut Context<DockArea>,
) -> PanelId {
    // Three concrete panel types, so the match cannot collapse into one
    // generic call - but every arm does the same two things in the same order,
    // and the id is taken from the entity before `add_panel` consumes it.
    match target {
        NavTarget::Logs => {
            let panel = cx.new(|cx| crate::logs::LogsPanel::new(context_name, cx));
            let id = PanelId::from(panel.entity_id());
            area.add_panel(panel, DockPlacement::Center, None, window, cx);
            id
        }
        NavTarget::Kind(kind) if has_concrete_panel(kind) => {
            let panel = cx.new(|cx| crate::pods::PodsPanel::new(context_name, cx));
            let id = PanelId::from(panel.entity_id());
            area.add_panel(panel, DockPlacement::Center, None, window, cx);
            id
        }
        NavTarget::Kind(kind) => {
            let panel = cx.new(|cx| {
                crate::placeholder::PlaceholderPanel::new(kind.clone(), context_name, cx)
            });
            let id = PanelId::from(panel.entity_id());
            area.add_panel(panel, DockPlacement::Center, None, window, cx);
            id
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NavTarget, SHOW_LOGS_COMMAND_ID, SHOW_PODS_COMMAND_ID, has_concrete_panel};
    use crate::cluster::discovery::DiscoveredKind;
    use crate::command::{CommandRegistry, build_items};
    use gpui_kit::TestAppContext;
    use kube::core::GroupVersionKind;

    fn kind(group: &str, kind: &str) -> DiscoveredKind {
        DiscoveredKind {
            gvk: GroupVersionKind::gvk(group, "v1", kind),
            plural: format!("{}s", kind.to_lowercase()),
            namespaced: true,
        }
    }

    /// Section 8.2: Pods is the one kind with a concrete panel, so it is the
    /// one kind that does not fall through to a placeholder.
    #[test]
    fn only_pods_has_a_concrete_panel() {
        assert!(has_concrete_panel(&DiscoveredKind::pods()));
        assert!(!has_concrete_panel(&kind("", "Service")));
        assert!(
            !has_concrete_panel(&kind("example.com", "Pod")),
            "a CRD's own Pod kind is not the built-in Pods panel"
        );
    }

    /// A CRD's `Pod` kind in its own group is a different kind from the built-in
    /// one, and must not be handed the Pods panel.
    #[test]
    fn a_pod_kind_in_another_group_is_not_the_pods_panel() {
        let crd_pod = kind("example.com", "Pod");
        assert!(!has_concrete_panel(&crd_pod));
    }

    /// Selecting Pods and Logs both name a real target, so the palette entries
    /// read the same as the Resource panel's rows.
    #[test]
    fn targets_label_themselves() {
        assert_eq!(NavTarget::pods().label(), "Pod");
        assert_eq!(NavTarget::Logs.label(), "Logs");
        assert_eq!(kind("apps", "Deployment").label(), "Deployment · apps");
    }

    /// Section 8.3: both panel-opening actions are registered, and both show up
    /// in the palette items built from the registry.
    #[test]
    fn panel_opening_actions_are_registered_commands() {
        let mut registry = CommandRegistry::new();
        super::register_commands(&mut registry);

        assert!(registry.get(SHOW_PODS_COMMAND_ID).is_some());
        assert!(registry.get(SHOW_LOGS_COMMAND_ID).is_some());
        assert!(registry.get("nav.does_not_exist").is_none());

        let titles: Vec<&str> = [SHOW_PODS_COMMAND_ID, SHOW_LOGS_COMMAND_ID]
            .iter()
            .map(|id| registry.get(id).expect("registered above").title)
            .collect();
        assert_eq!(titles, vec!["Show Pods", "Show Logs"]);
        assert_eq!(
            build_items(&registry, &[]).len(),
            2,
            "both commands reach the palette"
        );
    }

    /// Section 8.3: both panel-opening actions dispatch. A dispatched action
    /// is what either the palette or the keymap ends up producing, so this is
    /// the path a click and a keystroke share.
    #[gpui_kit::test]
    async fn both_panel_opening_actions_dispatch(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let mut registry = CommandRegistry::new();
        super::register_commands(&mut registry);

        let dispatched_pods = cx.update(|cx| registry.dispatch(SHOW_PODS_COMMAND_ID, &[], cx));
        let dispatched_logs = cx.update(|cx| registry.dispatch(SHOW_LOGS_COMMAND_ID, &[], cx));
        let dispatched = dispatched_pods && dispatched_logs;
        assert!(dispatched, "both commands are registered and ungated");

        assert!(!cx.update(|cx| registry.dispatch("nav.nope", &[], cx)));
    }
}
