//! Which panel a selected resource kind opens: the Pods panel for the core Pod
//! kind, and the generic list (`ObjectListPanel`) for every other discovered kind,
//! built-in or CRD (`standard-resource-panels` D4). The placeholder panel is only
//! a restore fallback now, for a saved panel whose kind is no longer served.

use crate::command::{Command, CommandRegistry};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::pod_detail::DetailView;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::dock::{DockArea, DockPlacement, PanelId, panel_handle};
use gpui_kit::*;

mod naming;
mod open_mode;
pub use open_mode::{OpenMode, OpenPodInBackground};

// `ShowPodDetail` and `ShowPodDetailYaml` are deliberately not registered
// commands: unlike the two above them they need a pod already selected
// (`SelectedPod`), so they are dispatched from within a Pods panel rather than
// offered as palette entries. They are two actions rather than one with an
// argument because `gpui_kit`'s actions are unit structs - and because the
// whole point of `y` is "the YAML, now": splitting the intent across an
// argument would leave the shortcut reaching the panel by the same route as
// `d`, and landing on the field list.
actions!(
    nav,
    [
        ShowPods,
        ShowLogs,
        ShowLogsFlipped,
        ShowEvents,
        ShowPodDetail,
        ShowPodDetailYaml
    ]
);

/// Opens the events browser for the window's active context (`events-browser`).
pub const SHOW_EVENTS_COMMAND_ID: &str = "nav.show_events";

pub const SHOW_PODS_COMMAND_ID: &str = "nav.show_pods";
pub const SHOW_PODS_DEFAULT_BINDING: &str = "cmd-1";
pub const SHOW_LOGS_COMMAND_ID: &str = "nav.show_logs";
pub const SHOW_LOGS_DEFAULT_BINDING: &str = "cmd-2";
/// `ShowLogs` the other way from the Logs panels preference, for one open
/// (`logs-panel-instancing`). Palette only here; the Pods list and a pod's
/// detail bind it to `shift-l` through their own commands.
pub const SHOW_LOGS_FLIPPED_COMMAND_ID: &str = "nav.show_logs_flipped";

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
    /// One object of any other discovered kind - the generic object viewer
    /// (`resource-links` section 5). Pod keeps its own variant and panel.
    Object(ObjectTarget),
    /// A shell in one container of a pod (`k9s-remaining-keybindings` 3).
    Exec(crate::k8s::resource::exec::ExecTarget),
    /// One pod's logs, in a Logs panel of its own (`logs-panel-instancing`):
    /// pinned to that pod, unlike [`Self::Logs`], which follows the selection.
    /// Keyed by the pod alone, as [`Self::Pod`] is - so opening the same pod's
    /// logs again focuses its panel, and another container of it switches
    /// that panel rather than opening a second.
    ///
    /// The variant names no cluster: two contexts' pods of the same namespace
    /// and name are the same `PodLogs`. What keeps their panels apart is
    /// `PanelKey.context_name` beside it - the panel's context, from
    /// `pod_scoped_context` - so dedup is per context, as for every target.
    PodLogs(PodRef),
    /// The logs of every pod a label selector picks (#150): a workload's pods,
    /// or a selector typed into the panel. Keyed by what it follows, so
    /// opening a workload's logs again focuses its panel, and the typed panel
    /// is one per context.
    LabelLogs(crate::util::logs::LabelLogs),
}

/// One object of a discovered kind: the kind as discovery reported it (so the
/// viewer knows the version, plural and scope to read it with), and which
/// object. `namespace` is `None` exactly when the kind is cluster-scoped.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ObjectTarget {
    pub kind: DiscoveredKind,
    pub namespace: Option<String>,
    pub name: String,
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

/// The commands this module contributes to the app-wide [`CommandRegistry`],
/// so the command palette and the Resource panel's rows dispatch the same
/// actions - see `shell::register_commands` for the sibling pattern.
///
/// Only the two panel-opening actions that exist without discovery can be
/// registered here. Neither is in the menu bar: Navigate holds only moving
/// focus and switching tabs (`menu-organization`), and Show Logs depends on
/// the selected pod. A per-kind command would have to be minted at runtime from
/// the cluster's kinds, and the palette is built from the registry at app
/// start - before any cluster is connected.
pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: SHOW_PODS_COMMAND_ID,
        title: "Show Pods",
        default_binding: SHOW_PODS_DEFAULT_BINDING,
        context: None,
        action: Box::new(ShowPods),
        menu: None,
    });
    registry.register(Command {
        id: SHOW_LOGS_COMMAND_ID,
        title: "Show Logs",
        default_binding: SHOW_LOGS_DEFAULT_BINDING,
        context: None,
        action: Box::new(ShowLogs),
        menu: None,
    });
    registry.register(Command {
        id: SHOW_LOGS_FLIPPED_COMMAND_ID,
        title: "Show Logs (Other Panel Mode)",
        default_binding: "",
        context: None,
        action: Box::new(ShowLogsFlipped),
        menu: None,
    });
    // Palette and keymap only: Navigate holds focus and tab moves.
    registry.register(Command {
        id: SHOW_EVENTS_COMMAND_ID,
        title: "Show Events",
        default_binding: "",
        context: None,
        action: Box::new(ShowEvents),
        menu: None,
    });
}

/// A panel `add_panel` just built, as the typed entity the window needs.
///
/// The window subscribes to this to hear a panel re-scope itself (10.2's
/// namespace picker), and `PanelKey` is built from the same
/// [`PanelScope`](crate::ui::panel_title::PanelScope) the panel was constructed
/// with. Returning the concrete type rather than a `PanelId` alone is what
/// makes that subscription possible; an erased handle could not be updated.
///
/// `Clone` because the window keeps a copy in `open_panels` to re-address a
/// panel it already built - `Entity` is a handle, so the copy shares the one
/// panel rather than duplicating it.
#[derive(Clone)]
pub enum OpenedPanel {
    Pods(Entity<crate::k8s::resource::pods::PodsPanel>),
    ObjectList(Entity<crate::k8s::resource::object_list::ObjectListPanel>),
    Events(Entity<crate::k8s::resource::events_browser::EventsPanel>),
    Placeholder(Entity<crate::ui::placeholder::PlaceholderPanel>),
    Logs(Entity<crate::util::logs::LogsPanel>),
    PodDetail(Entity<crate::k8s::resource::pod_detail::PodDetailPanel>),
    ObjectDetail(Entity<crate::k8s::resource::object_detail::ObjectDetailPanel>),
    Exec(Entity<crate::k8s::resource::exec::ExecPanel>),
}

impl OpenedPanel {
    /// Scopes the panel to `namespaces`, if it's a namespace-scoped list - a Pods,
    /// object, events or placeholder list. A detail or logs panel names its own
    /// object and keeps it. Whether it did.
    pub fn set_namespaces(&self, namespaces: Vec<String>, cx: &mut App) -> bool {
        match self {
            OpenedPanel::Pods(panel) => panel.update(cx, |p, cx| p.set_namespaces(namespaces, cx)),
            OpenedPanel::ObjectList(panel) => {
                panel.update(cx, |p, cx| p.set_namespaces(namespaces, cx))
            }
            OpenedPanel::Events(panel) => {
                panel.update(cx, |p, cx| p.set_namespaces(namespaces, cx))
            }
            OpenedPanel::Placeholder(panel) => {
                panel.update(cx, |p, cx| p.set_namespaces(namespaces, cx))
            }
            OpenedPanel::Logs(_)
            | OpenedPanel::PodDetail(_)
            | OpenedPanel::ObjectDetail(_)
            | OpenedPanel::Exec(_) => {
                return false;
            }
        }
        true
    }

    /// The dock id of the panel just built, without naming its type - what the
    /// window files it under and what `rescope` needs to find it again.
    pub fn panel_id(&self) -> PanelId {
        match self {
            OpenedPanel::Pods(panel) => PanelId::from(panel.entity_id()),
            OpenedPanel::ObjectList(panel) => PanelId::from(panel.entity_id()),
            OpenedPanel::Events(panel) => PanelId::from(panel.entity_id()),
            OpenedPanel::Placeholder(panel) => PanelId::from(panel.entity_id()),
            OpenedPanel::Logs(panel) => PanelId::from(panel.entity_id()),
            OpenedPanel::PodDetail(panel) => PanelId::from(panel.entity_id()),
            OpenedPanel::ObjectDetail(panel) => PanelId::from(panel.entity_id()),
            OpenedPanel::Exec(panel) => PanelId::from(panel.entity_id()),
        }
    }

    /// The panel's own focus handle - the one its root tracks. Test-only: the
    /// window finds focus through the dock's panels, which restored panels have too.
    #[cfg(test)]
    pub fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self {
            OpenedPanel::Pods(panel) => panel.read(cx).focus_handle(cx),
            OpenedPanel::ObjectList(panel) => panel.read(cx).focus_handle(cx),
            OpenedPanel::Events(panel) => panel.read(cx).focus_handle(cx),
            OpenedPanel::Placeholder(panel) => panel.read(cx).focus_handle(cx),
            OpenedPanel::Logs(panel) => panel.read(cx).focus_handle(cx),
            OpenedPanel::PodDetail(panel) => panel.read(cx).focus_handle(cx),
            OpenedPanel::ObjectDetail(panel) => panel.read(cx).focus_handle(cx),
            OpenedPanel::Exec(panel) => panel.read(cx).focus_handle(cx),
        }
    }
}

/// Recovers the typed handle behind `id`, for a panel this window did not itself
/// build - a restored one, whose only record in `util::shell::OpenPanel` is a
/// `panel: None` (see that field's doc comment). `area.panel(id)` hands back the
/// dock's own object-safe handle; `panel_name` says which of the four concrete panel
/// types it is, and `Entity::from(&dyn PanelView)` recovers it as that type.
///
/// `window-context-bar` section 3.3's Disconnect needs this: it must close every
/// panel a context has open in this window, including one the window only ever
/// restored from a saved dock layout and so has no [`OpenedPanel`] for yet.
pub fn opened_panel_for(
    area: &gpui_kit::component::dock::DockArea,
    id: PanelId,
    cx: &App,
) -> Option<OpenedPanel> {
    let view = area.panel(id)?;
    Some(match view.panel_name(cx) {
        "Pods" => OpenedPanel::Pods(Entity::from(view.as_ref())),
        "ObjectList" => OpenedPanel::ObjectList(Entity::from(view.as_ref())),
        "Events" => OpenedPanel::Events(Entity::from(view.as_ref())),
        "Logs" => OpenedPanel::Logs(Entity::from(view.as_ref())),
        "Resource" => OpenedPanel::Placeholder(Entity::from(view.as_ref())),
        "PodDetail" => OpenedPanel::PodDetail(Entity::from(view.as_ref())),
        "ObjectDetail" => OpenedPanel::ObjectDetail(Entity::from(view.as_ref())),
        "Exec" => OpenedPanel::Exec(Entity::from(view.as_ref())),
        _ => return None,
    })
}

/// Builds the panel for `scope` in its cluster's session and adds it to `area`'s
/// centre.
///
/// One code path for "open a panel", so a kind opened from the Resource panel,
/// from the context menu, or from `nav.show_pods` cannot drift apart. The
/// caller owns deduplication: whether this is a new panel or a focus of an
/// existing one is the window's bookkeeping, not the panel's.
///
/// `initial_view` is which view a panel that has two should open on - a pod's or
/// another object's detail panel - and only a YAML request (`y` on the Pods
/// table or a list) asks for anything but the default. It is a parameter rather than part of
/// [`NavTarget`] because the target is *identity*: the same target has to mean
/// "the same panel" whether the user described the pod or asked for its YAML,
/// or the two would dedup into two panels over one pod.
pub fn add_panel(
    area: &mut DockArea,
    scope: &PanelScope,
    initial_view: Option<DetailView>,
    window: &mut Window,
    cx: &mut Context<DockArea>,
) -> (PanelId, OpenedPanel) {
    // Three concrete panel types, so the match cannot collapse into one
    // generic call - but every arm does the same two things in the same order,
    // and the id is taken from the entity before `add_panel` consumes it.
    match &scope.target {
        NavTarget::Logs | NavTarget::PodLogs(_) | NavTarget::LabelLogs(_) => {
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
        NavTarget::Kind(kind) if kind.is_core_pod() => {
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
        // The core Event kind: the events browser rather than the generic list
        // (`events-browser` D4).
        NavTarget::Kind(kind) if kind.is_core_event() => {
            let panel = cx.new(|cx| {
                crate::k8s::resource::events_browser::EventsPanel::new(scope.clone(), cx)
            });
            let id = PanelId::from(panel.entity_id());
            area.add_panel_view(
                panel_handle(panel.clone()),
                DockPlacement::Center,
                None,
                window,
                cx,
            );
            (id, OpenedPanel::Events(panel))
        }
        // Every other kind, built-in or CRD: the generic live list.
        NavTarget::Kind(kind) => {
            let panel = cx.new(|cx| {
                crate::k8s::resource::object_list::ObjectListPanel::new(
                    kind.clone(),
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
            (id, OpenedPanel::ObjectList(panel))
        }
        // A pod's detail panel. Reads one pod through the cluster's existing
        // session, so it fetches that pod itself rather than joining a watch.
        // `initial_view` is how `y` lands straight on the YAML.
        NavTarget::Pod(pod) => {
            let panel = cx.new(|cx| {
                crate::k8s::resource::pod_detail::PodDetailPanel::new(
                    pod.clone(),
                    scope.clone(),
                    initial_view.unwrap_or_default(),
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
        // One object of any other discovered kind, read the same way.
        NavTarget::Object(object) => {
            let panel = cx.new(|cx| {
                crate::k8s::resource::object_detail::ObjectDetailPanel::new(
                    object.clone(),
                    scope.clone(),
                    cx,
                )
            });
            // `initial_view` is how a list's `y` lands straight on the YAML.
            if let Some(view) = initial_view {
                panel.update(cx, |panel, cx| panel.set_view(view, cx));
            }
            let id = PanelId::from(panel.entity_id());
            area.add_panel_view(
                panel_handle(panel.clone()),
                DockPlacement::Center,
                None,
                window,
                cx,
            );
            (id, OpenedPanel::ObjectDetail(panel))
        }
        // A shell in one container; its session starts as the panel opens.
        NavTarget::Exec(exec) => {
            let panel = cx.new(|cx| {
                crate::k8s::resource::exec::ExecPanel::new(exec.clone(), scope.clone(), window, cx)
            });
            let id = PanelId::from(panel.entity_id());
            area.add_panel_view(
                panel_handle(panel.clone()),
                DockPlacement::Center,
                None,
                window,
                cx,
            );
            (id, OpenedPanel::Exec(panel))
        }
    }
}

#[cfg(test)]
mod tests;
