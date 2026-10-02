//! The dock panel itself: its state, its lifecycle (fetch on connect), its
//! action handlers and its dock integration. What it draws is `render`'s and
//! `field_view`'s.

use super::commands::{
    SelectContainersTab, SelectEventsTab, SelectManagedFieldsTab, SelectOverviewTab,
    SelectVolumesTab, ToggleDetailView,
};
use super::fetch::{PodDetailState, PodFetch, fetch_pod};
use super::fields::pod_fields;
use super::model::{DetailSection, DetailView, PodField};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::discovery_registry::{DiscoveredKinds, DiscoveryRegistry};
use crate::ui::link::{self, GoToEntry, GoToReference};
use crate::ui::nav::{NavTarget, PodRef};
use crate::ui::panel_title::{self, PanelScope};
use gpui_kit::component::dock::{
    BasePanel, Panel, PanelControl, PanelEvent, PanelInfo, PanelState, panel_handle, register_panel,
};
use gpui_kit::*;
use jiff::Timestamp;
use k8s_openapi::api::core::v1::Pod;

pub fn register_restore(cx: &mut gpui_kit::App) {
    register_panel(cx, "PodDetail", |context, _window, cx| {
        use crate::ui::unrestored::{required_str, restore_with};
        restore_with(&context, cx, |state, cx| {
            let context_name = required_str(state, "context_name")?.to_string();
            let namespace = required_str(state, "pod_namespace")?.to_string();
            let name = required_str(state, "pod_name")?.to_string();
            let target = NavTarget::pod(namespace.clone(), name.clone());
            let scope = PanelScope::new(target, context_name);
            Ok(panel_handle(cx.new(|cx| {
                PodDetailPanel::new(
                    PodRef { namespace, name },
                    scope,
                    DetailView::Structured,
                    cx,
                )
            })))
        })
    });
}

/// A dock panel showing one Pod's structured fields, or its raw YAML.
pub struct PodDetailPanel {
    /// Which pod this panel is over - also the identity `PanelKey` dedups on,
    /// through the scope's `NavTarget::Pod`.
    pub(super) pod: PodRef,
    pub(super) scope: PanelScope,
    pub(super) connection: Entity<ClusterConnection>,
    /// The context's discovered kinds, which decide which references are
    /// links. Observed, so references turn into links when discovery lands.
    pub(super) discovery: Entity<DiscoveredKinds>,
    pub(super) state: PodDetailState,
    pub(super) viewing: DetailView,
    /// Which tab of the structured view is showing. Irrelevant while
    /// `viewing` is `Yaml`, but kept regardless so switching back to
    /// Structured returns to the tab the user left, not always Overview.
    pub(super) active_tab: DetailSection,
    /// Which disclosures are expanded: `Collapsed`-value fields (Tolerations)
    /// keyed by field label, Managed Fields entries keyed `mf-<index>`, and
    /// container cards keyed `container:<name>`.
    /// Absent means collapsed - the default for a long list the user came for
    /// something else in.
    pub(super) open_sections: std::collections::HashSet<String>,
    /// The YAML view's folds and scroll position.
    pub(super) yaml_view: crate::ui::yaml_view::YamlViewState,
    /// Whether a fetch is in flight, so a connection that flaps does not race
    /// two results into `state`.
    pub(super) fetching: bool,
    /// The pod's live events, once it has loaded (`live_events`).
    pub(super) events: Option<super::live_events::PodEventsWatch>,
    /// How far back the Events tab looks (`pod-events-time-window` 2.1).
    pub(super) events_window: crate::config::ui::PodEventsWindow,
    /// The Configuration tab's cards and revealed Secret values. Revealed
    /// values live only here, and only until hidden, the tab is left, or the
    /// panel closes.
    pub(super) configuration: super::configuration::ConfigurationState,
    pub(super) focus_handle: FocusHandle,
}

impl PodDetailPanel {
    pub fn new(pod: PodRef, scope: PanelScope, view: DetailView, cx: &mut Context<Self>) -> Self {
        use crate::k8s::cluster::session::ClusterRegistry;

        let connection = ClusterRegistry::connection(cx, &scope.context_name);
        cx.observe(&connection, |this: &mut Self, _, cx| this.sync(cx))
            .detach();
        let discovery = DiscoveryRegistry::kinds(cx, &scope.context_name);
        cx.observe(&discovery, |_, _, cx| cx.notify()).detach();

        let mut this = Self {
            pod,
            scope,
            connection,
            discovery,
            state: PodDetailState::Loading,
            configuration: Default::default(),
            viewing: view,
            active_tab: DetailSection::Overview,
            open_sections: std::collections::HashSet::new(),
            yaml_view: Default::default(),
            fetching: false,
            events: None,
            events_window: super::window_preference::preferred(cx),
            focus_handle: crate::ui::panel::focus::panel_focus_handle(cx),
        };
        this.sync(cx);
        this
    }

    /// Construction from an explicit connection, so tests can hand in a stub
    /// instead of the real registry - the same seam `ResourcePanel` uses. The
    /// stub stays non-`Connected`, so nothing spawns a real connect task.
    #[cfg(test)]
    pub(super) fn with_connection(
        pod: PodRef,
        scope: PanelScope,
        view: DetailView,
        connection: Entity<ClusterConnection>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&connection, |this: &mut Self, _, cx| this.sync(cx))
            .detach();
        let discovery = cx.new(|_| DiscoveredKinds::loaded(Vec::new()));
        let mut this = Self {
            pod,
            scope,
            connection,
            discovery,
            state: PodDetailState::Loading,
            configuration: Default::default(),
            viewing: view,
            active_tab: DetailSection::Overview,
            open_sections: std::collections::HashSet::new(),
            yaml_view: Default::default(),
            fetching: false,
            events: None,
            events_window: super::window_preference::preferred(cx),
            focus_handle: crate::ui::panel::focus::panel_focus_handle(cx),
        };
        this.sync(cx);
        this
    }

    /// Fetches the pod once its cluster has a client. A no-op while a fetch is
    /// in flight or the connection is not up yet - the observer on the
    /// connection calls this again once it is.
    pub(super) fn sync(&mut self, cx: &mut Context<Self>) {
        if self.fetching {
            return;
        }
        let ConnectionState::Connected(client) = &self.connection.read(cx).state else {
            return;
        };
        let client = client.clone();
        let namespace = self.pod.namespace.clone();
        let name = self.pod.name.clone();
        self.fetching = true;
        let watch_client = client.clone();
        let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
            let _ = tx.send(fetch_pod(client, namespace, name).await).await;
        });
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, |result| {
                let _ = this.update(cx, |this, cx| {
                    this.fetching = false;
                    this.state = match result {
                        Ok(PodFetch::Found(pod)) => {
                            this.watch_events(&pod, watch_client.clone(), cx);
                            PodDetailState::Loaded(pod)
                        }
                        Ok(PodFetch::NotFound) => PodDetailState::NotFound,
                        Err((message, detail)) => PodDetailState::Failed { message, detail },
                    };
                    // Opened straight onto the Configuration tab (or still on
                    // it across a refetch): load its cards now the pod is in.
                    if this.active_tab == DetailSection::Configuration {
                        this.ensure_configuration_loaded(cx);
                    }
                    cx.notify();
                });
            })
            .await;
        })
        .detach();
    }

    /// Switches the view. Called by the toolbar toggle, and by `MainWindow`
    /// when `nav.show_pod_detail_yaml` is dispatched for a panel that is
    /// already open - `y` has to land on the YAML even when the panel exists.
    pub fn set_view(&mut self, view: DetailView, cx: &mut Context<Self>) {
        self.viewing = view;
        cx.notify();
    }

    /// Which view is showing. The assertion hook for the window's
    /// switch-on-focus path, which cannot read `viewing` across the module
    /// boundary - hence test-only, like [`Self::with_connection`].
    #[cfg(test)]
    pub fn view(&self) -> DetailView {
        self.viewing
    }

    /// Lands `pod` as if a fetch had returned it, for tests outside this module
    /// that need a loaded panel without a cluster (the window's link tests).
    #[cfg(test)]
    pub(crate) fn test_set_loaded(&mut self, pod: Pod, cx: &mut Context<Self>) {
        self.state = PodDetailState::Loaded(Box::new(pod));
        cx.notify();
    }

    /// Switches the active tab of the structured view.
    pub(super) fn set_active_tab(&mut self, section: DetailSection, cx: &mut Context<Self>) {
        // Leaving the Configuration tab hides every revealed value - one
        // doesn't sit on screen behind a tab switch - and collapses every
        // value, so the tab is shown again collapsed.
        if self.active_tab == DetailSection::Configuration && section != self.active_tab {
            self.configuration.leave();
        }
        self.active_tab = section;
        if section == DetailSection::Configuration {
            self.ensure_configuration_loaded(cx);
        }
        cx.notify();
    }

    /// Switches tab from the keyboard, keeping keyboard focus in the panel: a
    /// focused control inside the old tab (a reveal button) unmounts with it,
    /// which would otherwise leave nothing focused and the next key dead.
    pub(super) fn switch_tab(
        &mut self,
        section: DetailSection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let had_focus = self.focus_handle.contains_focused(window, cx);
        self.set_active_tab(section, cx);
        if had_focus {
            self.focus_handle.focus(window, cx);
        }
    }

    /// The active tab. Test-only, like [`Self::view`].
    #[cfg(test)]
    pub fn active_tab(&self) -> DetailSection {
        self.active_tab
    }

    /// The loaded pod, if the fetch has landed. Read by tests and by render.
    pub(super) fn pod(&self) -> Option<&Pod> {
        match &self.state {
            PodDetailState::Loaded(pod) => Some(pod),
            _ => None,
        }
    }

    /// The rows the structured view renders, or empty while there is nothing to
    /// show. Projected here rather than in `render` so a test asserts the rows
    /// the panel would draw.
    pub(super) fn fields(&self, now: Timestamp) -> Vec<PodField> {
        self.pod()
            .map(|pod| pod_fields(pod, now))
            .unwrap_or_default()
    }

    /// The references the "Go to…" picker offers: every followable one the
    /// structured view shows. Empty while nothing has loaded, which is also
    /// what hides the `g` hint.
    pub(super) fn followable(&self, cx: &App) -> Vec<GoToEntry> {
        link::followable(
            super::references::go_to_entries(&self.fields(Timestamp::now())),
            self.kinds(cx),
        )
    }

    /// The discovered kinds references are resolved against.
    pub(super) fn kinds<'a>(&self, cx: &'a App) -> Option<&'a [DiscoveredKind]> {
        self.discovery.read(cx).kinds()
    }

    /// Replaces the discovered kinds, for tests that need a kind to be
    /// followable.
    #[cfg(test)]
    pub(crate) fn test_set_kinds(&mut self, kinds: Vec<DiscoveredKind>, cx: &mut Context<Self>) {
        self.discovery = cx.new(|_| DiscoveredKinds::loaded(kinds));
        cx.notify();
    }

    /// The raw manifest, or nothing while there is no pod to render. Shared by
    /// the YAML view and its test, so the test reads exactly what the view
    /// draws rather than a second serialization that could drift from it.
    pub(super) fn yaml(&self) -> Option<String> {
        self.pod()
            .and_then(|pod| serde_yaml_ng::to_string(pod).ok())
    }
}

impl Focusable for PodDetailPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for PodDetailPanel {}

impl PodDetailPanel {
    pub(super) fn on_action_go_to(
        &mut self,
        _: &GoToReference,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entries = self.followable(cx);
        if entries.is_empty() {
            return;
        }
        link::open_go_to(
            entries,
            self.scope.context_name.clone(),
            self.focus_handle.clone(),
            window,
            cx,
        );
    }

    pub(super) fn on_action_toggle_view(
        &mut self,
        _: &ToggleDetailView,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = match self.viewing {
            DetailView::Structured => DetailView::Yaml,
            DetailView::Yaml => DetailView::Structured,
        };
        self.set_view(next, cx);
    }

    pub(super) fn on_action_select_overview_tab(
        &mut self,
        _: &SelectOverviewTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_tab(DetailSection::Overview, window, cx);
    }

    pub(super) fn on_action_select_containers_tab(
        &mut self,
        _: &SelectContainersTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_tab(DetailSection::Containers, window, cx);
    }

    pub(super) fn on_action_select_volumes_tab(
        &mut self,
        _: &SelectVolumesTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_tab(DetailSection::Volumes, window, cx);
    }

    pub(super) fn on_action_select_events_tab(
        &mut self,
        _: &SelectEventsTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_tab(DetailSection::Events, window, cx);
    }

    pub(super) fn on_action_select_managed_fields_tab(
        &mut self,
        _: &SelectManagedFieldsTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_tab(DetailSection::ManagedFields, window, cx);
    }
}

impl BasePanel for PodDetailPanel {
    fn panel_name(&self) -> &'static str {
        "PodDetail"
    }

    fn dump(&self, _cx: &App) -> PanelState {
        PanelState {
            panel_name: self.panel_name().to_string(),
            children: Vec::new(),
            info: PanelInfo::Panel(serde_json::json!({
                "context_name": self.scope.context_name,
                "pod_namespace": self.pod.namespace,
                "pod_name": self.pod.name,
            })),
        }
    }
}

/// The title bar, supplied to the dock rather than drawn here - the same bar
/// every other panel gets, so `"Pod: <name>"` comes from section 2's rule
/// rather than a second implementation in this panel.
///
/// No namespace picker: the panel is over one pod, not a namespace-scoped list,
/// so there is no scope for a picker to change.
impl Panel for PodDetailPanel {
    fn title(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        panel_title::title_element(
            &self.scope,
            panel_title::title(&self.scope),
            &self.focus_handle,
            panel_title::close_button(cx.entity()),
            window,
            cx,
        )
    }

    fn tab_name(&self, _cx: &App) -> Option<SharedString> {
        panel_title::tab_name(&self.scope)
    }

    fn zoom_control(&self, _cx: &App) -> Option<PanelControl> {
        Some(PanelControl::Toolbar)
    }
}
