//! Which panel a selected resource kind opens. Section 8.2 of the
//! `cluster-picker-and-navigation` change: a kind with a concrete panel gets
//! it, and every other discovered kind gets a placeholder, so the Resource
//! panel never offers a row that opens nothing.

use crate::command::{Command, CommandRegistry};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::ui::panel_title::PanelScope;
use gpui_kit::assets::IconName;
use gpui_kit::component::dock::{DockArea, DockPlacement, PanelId, panel_handle};
use gpui_kit::*;

// `ShowPodDetail` is deliberately not a registered command: unlike the two
// above it needs a pod already selected (`SelectedPod`), so it is dispatched
// from within a Pods panel rather than offered as a palette entry.
actions!(nav, [ShowPods, ShowLogs, ShowPodDetail]);

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
    /// One specific pod, rather than the list of pods its kind would be.
    ///
    /// `PanelKey` is built from the target, so this variant is what separates
    /// two pods' detail panels in one dock the same way `Kind` separates two
    /// different kinds: by being a different key, with no dedup logic of its
    /// own. `Kind(DiscoveredKind::pods())` and this are never the same panel,
    /// which is the point - one is the list, the other a row of it.
    Pod(PodRef),
}

/// A pod's identity, at the granularity a panel keyed on it needs: which pod
/// in which namespace. Deliberately not the whole [`PodSelection`] - that
/// carries the container list the Logs view streams, which no detail panel
/// needs and which would make a pod's panel key change as its containers do.
///
/// [`PodSelection`]: crate::k8s::resource::pods::PodSelection
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PodRef {
    pub namespace: String,
    pub name: String,
}

impl NavTarget {
    /// The target of the `nav.show_pods` command. Built without discovery, so
    /// it is pinned to the core `v1` `Pod` kind every cluster reports.
    pub fn pods() -> Self {
        NavTarget::Kind(DiscoveredKind::pods())
    }

    /// The detail view over one pod.
    pub fn pod(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        NavTarget::Pod(PodRef {
            namespace: namespace.into(),
            name: name.into(),
        })
    }

    /// The kind this target shows, for the cases that read it as one. A pod's
    /// detail panel is over the core `Pod` kind, pinned the same way
    /// [`Self::pods`] pins the list's.
    fn pod_kind() -> DiscoveredKind {
        DiscoveredKind::pods()
    }

    pub fn label(&self) -> String {
        match self {
            NavTarget::Kind(kind) => kind.label(),
            NavTarget::Logs => "Logs".to_string(),
            NavTarget::Pod(_) => Self::pod_kind().label(),
        }
    }

    /// What a panel *listing* this target titles itself: the plural form for
    /// a resource kind (`"Pods"`), same as [`Self::label`] for anything that
    /// isn't a list of many items.
    pub fn list_label(&self) -> String {
        match self {
            NavTarget::Kind(kind) => kind.plural_label(),
            NavTarget::Logs | NavTarget::Pod(_) => self.label(),
        }
    }

    /// What a panel titles itself when it shows *one* item rather than a list of
    /// them: the kind's singular name, plus the item's own name so two panels
    /// over different pods are told apart in the dock's tabs.
    pub fn item_label(&self) -> String {
        match self {
            NavTarget::Pod(pod) => format!("{}: {}", self.label(), pod.name),
            _ => self.list_label(),
        }
    }

    pub fn icon(&self) -> IconName {
        match self {
            NavTarget::Kind(_) | NavTarget::Pod(_) => IconName::Box,
            NavTarget::Logs => IconName::ScrollText,
        }
    }
}

/// Whether this build has a concrete panel for `kind`. Pods is the only one
/// today; everything else falls through to
/// [`crate::ui::placeholder::PlaceholderPanel`].
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

/// A panel `add_panel` just built, as the typed entity the window needs.
///
/// The window subscribes to this to hear a panel re-scope itself (10.2's
/// namespace picker), and `PanelKey` is built from the same
/// [`PanelScope`](crate::ui::panel_title::PanelScope) the panel was constructed
/// with. Returning the concrete type rather than a `PanelId` alone is what
/// makes that subscription possible; an erased handle could not be updated.
pub enum OpenedPanel {
    Pods(Entity<crate::k8s::resource::pods::PodsPanel>),
    Placeholder(Entity<crate::ui::placeholder::PlaceholderPanel>),
    Logs(Entity<crate::util::logs::LogsPanel>),
    PodDetail(Entity<crate::k8s::resource::pod_detail::PodDetailPanel>),
}

impl OpenedPanel {
    /// The dock id of the panel just built, without naming its type - what the
    /// window files it under and what `rescope` needs to find it again.
    pub fn panel_id(&self) -> PanelId {
        match self {
            OpenedPanel::Pods(panel) => PanelId::from(panel.entity_id()),
            OpenedPanel::Placeholder(panel) => PanelId::from(panel.entity_id()),
            OpenedPanel::Logs(panel) => PanelId::from(panel.entity_id()),
            OpenedPanel::PodDetail(panel) => PanelId::from(panel.entity_id()),
        }
    }
}

/// Builds the panel for `scope` in its cluster's session and adds it to `area`'s
/// centre.
///
/// One code path for "open a panel", so a kind opened from the Resource panel,
/// from the context menu, or from `nav.show_pods` cannot drift apart. The
/// caller owns deduplication: whether this is a new panel or a focus of an
/// existing one is the window's bookkeeping, not the panel's.
pub fn add_panel(
    area: &mut DockArea,
    scope: &PanelScope,
    window: &mut Window,
    cx: &mut Context<DockArea>,
) -> (PanelId, OpenedPanel) {
    // Three concrete panel types, so the match cannot collapse into one
    // generic call - but every arm does the same two things in the same order,
    // and the id is taken from the entity before `add_panel` consumes it.
    match &scope.target {
        NavTarget::Logs => {
            let panel = cx.new(|cx| crate::util::logs::LogsPanel::new(scope.clone(), cx));
            let id = PanelId::from(panel.entity_id());
            area.add_panel_view(
                panel_handle(panel.clone()),
                DockPlacement::Center,
                None,
                window,
                cx,
            );
            (id, OpenedPanel::Logs(panel))
        }
        NavTarget::Kind(kind) if has_concrete_panel(kind) => {
            let panel = cx.new(|cx| crate::k8s::resource::pods::PodsPanel::new(scope.clone(), cx));
            let id = PanelId::from(panel.entity_id());
            area.add_panel_view(
                panel_handle(panel.clone()),
                DockPlacement::Center,
                None,
                window,
                cx,
            );
            (id, OpenedPanel::Pods(panel))
        }
        NavTarget::Kind(kind) => {
            let panel = cx.new(|cx| {
                crate::ui::placeholder::PlaceholderPanel::new(kind.clone(), scope.clone(), cx)
            });
            let id = PanelId::from(panel.entity_id());
            area.add_panel_view(
                panel_handle(panel.clone()),
                DockPlacement::Center,
                None,
                window,
                cx,
            );
            (id, OpenedPanel::Placeholder(panel))
        }
        // A pod's detail panel. Reads one pod through the cluster's existing
        // session, so it fetches that pod itself rather than joining a watch.
        NavTarget::Pod(pod) => {
            let panel = cx.new(|cx| {
                crate::k8s::resource::pod_detail::PodDetailPanel::new(
                    pod.clone(),
                    scope.clone(),
                    cx,
                )
            });
            let id = PanelId::from(panel.entity_id());
            area.add_panel_view(
                panel_handle(panel.clone()),
                DockPlacement::Center,
                None,
                window,
                cx,
            );
            (id, OpenedPanel::PodDetail(panel))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NavTarget, SHOW_LOGS_COMMAND_ID, SHOW_PODS_COMMAND_ID, has_concrete_panel};
    use crate::command::{CommandRegistry, build_items};
    use crate::k8s::cluster::discovery::DiscoveredKind;
    use gpui_kit::TestAppContext;
    use kube::core::GroupVersionKind;

    fn kind(group: &str, kind: &str) -> DiscoveredKind {
        DiscoveredKind {
            gvk: GroupVersionKind::gvk(group, "v1", kind),
            plural: format!("{}s", kind.to_lowercase()),
            namespaced: true,
        }
    }

    /// Section 1.1: a pod's detail panel is keyed on *which* pod, so two
    /// different pods are two different targets and the same pod is one. The
    /// equality is what `PanelKey`'s dedup reads - same key focuses, different
    /// key opens a second panel - so it has to be exactly this.
    #[test]
    fn two_pods_are_different_targets_and_one_pod_is_one_target() {
        let first = NavTarget::pod("default", "web-1");
        let second = NavTarget::pod("default", "web-2");
        let same = NavTarget::pod("default", "web-1");

        assert_ne!(first, second, "different pods are different panels");
        assert_eq!(first, same, "the same pod is the same panel");
    }

    /// A pod's namespace is part of its identity: the same name in two
    /// namespaces is two pods, and collapsing them would make a second cluster
    /// namespace's panel unreachable behind the first one's.
    #[test]
    fn a_pods_namespace_is_part_of_its_identity() {
        assert_ne!(
            NavTarget::pod("default", "web-1"),
            NavTarget::pod("staging", "web-1")
        );
    }

    /// The list of pods and one pod are not the same panel, whichever way round
    /// they are compared - a detail request must never focus the Pods table.
    #[test]
    fn a_pod_is_not_the_list_of_pods() {
        assert_ne!(NavTarget::pods(), NavTarget::pod("default", "web-1"));
        assert_ne!(NavTarget::pod("default", "web-1"), NavTarget::pods());
    }

    /// Section 2.1/2.2, read off the target itself: a list titles itself
    /// plural, a single item titles itself with the item's name.
    #[test]
    fn a_target_says_which_way_it_should_be_titled() {
        assert_eq!(NavTarget::pods().list_label(), "Pods");
        assert_eq!(NavTarget::pods().item_label(), "Pods");

        let pod = NavTarget::pod("default", "api-7d9f-ftg5t");
        assert_eq!(pod.label(), "Pod");
        assert_eq!(pod.item_label(), "Pod: api-7d9f-ftg5t");
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
