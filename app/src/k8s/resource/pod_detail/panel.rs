//! The dock panel itself: its state, its lifecycle (fetch on connect), its
//! action handlers and its dock integration. What it draws is `render`'s and
//! `field_view`'s.

use super::commands::{
    SelectContainersTab, SelectEventsTab, SelectManagedFieldsTab, SelectOverviewTab,
    SelectVolumesTab, ToggleDetailView,
};
use super::fetch::{PodDetailState, PodEvents, PodFetch, fetch_pod};
use super::fields::pod_fields;
use super::model::{DetailSection, DetailView, PodField};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::ui::nav::{NavTarget, PodRef};
use crate::ui::panel_title::{self, PanelScope};
use gpui_kit::component::button::Button;
use gpui_kit::component::dock::{
    BasePanel, Panel, PanelControl, PanelEvent, PanelInfo, PanelState, panel_handle, register_panel,
};
use gpui_kit::*;
use jiff::Timestamp;
use k8s_openapi::api::core::v1::Pod;

pub fn register_restore(cx: &mut gpui_kit::App) {
    register_panel(cx, "PodDetail", |context, _window, cx| {
        let PanelInfo::Panel(state) = context.info() else {
            panic!("PodDetail layout state must be a panel");
        };
        let context_name = state["context_name"]
            .as_str()
            .expect("PodDetail layout state must name its cluster")
            .to_string();
        let namespace = state["pod_namespace"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let name = state["pod_name"].as_str().unwrap_or_default().to_string();
        let target = NavTarget::pod(namespace.clone(), name.clone());
        let scope = PanelScope::new(target, context_name);
        panel_handle(cx.new(|cx| {
            PodDetailPanel::new(
                PodRef { namespace, name },
                scope,
                DetailView::Structured,
                cx,
            )
        }))
    });
}

/// A dock panel showing one Pod's structured fields, or its raw YAML.
pub struct PodDetailPanel {
    /// Which pod this panel is over - also the identity `PanelKey` dedups on,
    /// through the scope's `NavTarget::Pod`.
    pub(super) pod: PodRef,
    pub(super) scope: PanelScope,
    pub(super) connection: Entity<ClusterConnection>,
    pub(super) state: PodDetailState,
    pub(super) viewing: DetailView,
    /// Which tab of the structured view is showing. Irrelevant while
    /// `viewing` is `Yaml`, but kept regardless so switching back to
    /// Structured returns to the tab the user left, not always Overview.
    pub(super) active_tab: DetailSection,
    /// Which disclosures are expanded: `Collapsed`-value fields (Tolerations)
    /// keyed by field label, and Managed Fields entries keyed `mf-<index>`.
    /// Absent means collapsed - the default for a long list the user came for
    /// something else in.
    pub(super) open_sections: std::collections::HashSet<String>,
    /// Whether a fetch is in flight, so a connection that flaps does not race
    /// two results into `state`.
    pub(super) fetching: bool,
    pub(super) focus_handle: FocusHandle,
}

impl PodDetailPanel {
    pub fn new(pod: PodRef, scope: PanelScope, view: DetailView, cx: &mut Context<Self>) -> Self {
        use crate::k8s::cluster::session::ClusterRegistry;

        let connection = ClusterRegistry::connection(cx, &scope.context_name);
        cx.observe(&connection, |this: &mut Self, _, cx| this.sync(cx))
            .detach();

        let mut this = Self {
            pod,
            scope,
            connection,
            state: PodDetailState::Loading,
            viewing: view,
            active_tab: DetailSection::Overview,
            open_sections: std::collections::HashSet::new(),
            fetching: false,
            focus_handle: cx.focus_handle(),
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
        let mut this = Self {
            pod,
            scope,
            connection,
            state: PodDetailState::Loading,
            viewing: view,
            active_tab: DetailSection::Overview,
            open_sections: std::collections::HashSet::new(),
            fetching: false,
            focus_handle: cx.focus_handle(),
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
        let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
            let _ = tx.send(fetch_pod(client, namespace, name).await).await;
        });
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, |result| {
                let _ = this.update(cx, |this, cx| {
                    this.fetching = false;
                    this.state = match result {
                        Ok(PodFetch::Found(pod, events)) => PodDetailState::Loaded(pod, events),
                        Ok(PodFetch::NotFound) => PodDetailState::NotFound,
                        Err((message, detail)) => PodDetailState::Failed { message, detail },
                    };
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
        self.state = PodDetailState::Loaded(Box::new(pod), Ok(Vec::new()));
        cx.notify();
    }

    /// Switches the active tab of the structured view.
    pub(super) fn set_active_tab(&mut self, section: DetailSection, cx: &mut Context<Self>) {
        self.active_tab = section;
        cx.notify();
    }

    /// The active tab. Test-only, like [`Self::view`].
    #[cfg(test)]
    pub fn active_tab(&self) -> DetailSection {
        self.active_tab
    }

    /// The loaded pod, if the fetch has landed. Read by tests and by render.
    pub(super) fn pod(&self) -> Option<&Pod> {
        match &self.state {
            PodDetailState::Loaded(pod, _) => Some(pod),
            _ => None,
        }
    }

    /// The events naming this pod, once the fetch has landed - or why they
    /// could not be listed. `None` until then.
    pub(super) fn events(&self) -> Option<&PodEvents> {
        match &self.state {
            PodDetailState::Loaded(_, events) => Some(events),
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
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_active_tab(DetailSection::Overview, cx);
    }

    pub(super) fn on_action_select_containers_tab(
        &mut self,
        _: &SelectContainersTab,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_active_tab(DetailSection::Containers, cx);
    }

    pub(super) fn on_action_select_volumes_tab(
        &mut self,
        _: &SelectVolumesTab,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_active_tab(DetailSection::Volumes, cx);
    }

    pub(super) fn on_action_select_events_tab(
        &mut self,
        _: &SelectEventsTab,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_active_tab(DetailSection::Events, cx);
    }

    pub(super) fn on_action_select_managed_fields_tab(
        &mut self,
        _: &SelectManagedFieldsTab,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_active_tab(DetailSection::ManagedFields, cx);
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
    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        panel_title::title_element(&self.scope, panel_title::title(&self.scope))
    }

    fn tab_name(&self, _cx: &App) -> Option<SharedString> {
        panel_title::tab_name(&self.scope)
    }

    fn toolbar_buttons(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Vec<Button>> {
        panel_title::toolbar_buttons()
    }

    fn zoom_control(&self, _cx: &App) -> Option<PanelControl> {
        Some(PanelControl::Toolbar)
    }
}
