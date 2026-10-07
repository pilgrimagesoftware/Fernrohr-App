//! The dock panel's state and lifecycle: [`LogsPanel`] itself, restoring it
//! from a saved layout, and (re)starting its log stream as the app-scoped
//! selected pod or its cluster connection changes. What it draws is
//! `render`'s and `title`'s.

use super::*;

pub fn register_restore(cx: &mut App) {
    register_panel(cx, "Logs", |context, _window, cx| {
        crate::ui::unrestored::restore_with(&context, cx, |state, cx| {
            let context_name =
                crate::ui::unrestored::required_str(state, "context_name")?.to_string();
            let namespaces =
                serde_json::from_value(state["namespaces"].clone()).unwrap_or_default();
            // A pod's own panel saves its pod and container, and comes back
            // pinned to them; the one shared panel saves neither.
            match pinned_from_state(state, &context_name) {
                Some(pinned) => {
                    let pod = crate::ui::nav::PodRef {
                        namespace: pinned.namespace.clone(),
                        name: pinned.name.clone(),
                    };
                    let scope = PanelScope::new(NavTarget::PodLogs(pod), context_name)
                        .scoped_to(namespaces);
                    Ok(panel_handle(
                        cx.new(|cx| LogsPanel::build(scope, Some(pinned), cx)),
                    ))
                }
                None => {
                    let scope =
                        PanelScope::new(NavTarget::Logs, context_name).scoped_to(namespaces);
                    Ok(panel_handle(cx.new(|cx| LogsPanel::new(scope, cx))))
                }
            }
        })
    });
}

/// The pod (and the container it showed) a pod's own Logs panel saved, or
/// `None` for the shared panel.
pub(crate) fn pinned_from_state(
    state: &serde_json::Value,
    context_name: &str,
) -> Option<PodSelection> {
    Some(PodSelection {
        namespace: state["pod_namespace"].as_str()?.to_string(),
        name: state["pod_name"].as_str()?.to_string(),
        containers: state["container"]
            .as_str()
            .map(|container| vec![container.to_string()])
            .unwrap_or_default(),
        context_name: context_name.to_string(),
    })
}

/// A dock panel streaming container logs: of whichever pod was last clicked
/// in a Pods panel (see [`SelectedPod`]), or - a pod's own panel
/// (`NavTarget::PodLogs`) - of that one pod, whatever is selected since.
pub struct LogsPanel {
    pub(super) scope: PanelScope,
    /// The pod a pod's own panel is pinned to, and its containers in the
    /// order the selection gave them (the first is the one shown); `None`
    /// for the shared panel, which follows [`SelectedPod`].
    pub(super) pinned: Option<PodSelection>,
    connection: Entity<crate::k8s::cluster::connection::ClusterConnection>,
    pub(super) view: Entity<LogsView>,
    stream: Option<Task<()>>,
    pub(super) current: Option<(String, String, String)>,
    /// Whether the panel shows the container's previous instance's logs
    /// rather than its current one's (`k9s-remaining-keybindings` 5).
    pub(super) previous: bool,
    pub(super) scroll_handle: UniformListScrollHandle,
    pub(super) focus_handle: FocusHandle,
}

impl LogsPanel {
    /// The panel for `scope`: a pod's own, pinned to that pod as the selection
    /// names it now, for `NavTarget::PodLogs`; else the shared one.
    pub fn new(scope: PanelScope, cx: &mut Context<Self>) -> Self {
        let pinned = match &scope.target {
            NavTarget::PodLogs(pod) => Some(
                cx.try_global::<SelectedPod>()
                    .and_then(|selected| selected.0.clone())
                    .filter(|selection| {
                        selection.context_name == scope.context_name
                            && selection.namespace == pod.namespace
                            && selection.name == pod.name
                    })
                    .unwrap_or_else(|| PodSelection {
                        namespace: pod.namespace.clone(),
                        name: pod.name.clone(),
                        containers: Vec::new(),
                        context_name: scope.context_name.clone(),
                    }),
            ),
            _ => None,
        };
        Self::build(scope, pinned, cx)
    }

    /// The panel over `scope`, pinned to `pinned` when it is a pod's own. Only
    /// the shared panel observes [`SelectedPod`].
    pub(crate) fn build(
        scope: PanelScope,
        pinned: Option<PodSelection>,
        cx: &mut Context<Self>,
    ) -> Self {
        use crate::k8s::cluster::session::ClusterRegistry;

        let connection = ClusterRegistry::connection(cx, &scope.context_name);
        cx.observe(&connection, |this: &mut Self, _, cx| this.sync(cx))
            .detach();
        if pinned.is_none() {
            cx.observe_global::<SelectedPod>(|this: &mut Self, cx| this.sync(cx))
                .detach();
        }

        let mut this = Self {
            scope,
            pinned,
            connection,
            view: cx.new(|_| LogsView::new(vec![String::new()])),
            stream: None,
            current: None,
            previous: false,
            scroll_handle: UniformListScrollHandle::default(),
            focus_handle: crate::ui::panel::focus::panel_focus_handle(cx),
        };
        this.sync(cx);
        this
    }

    /// Starts (or restarts) the stream if the selected pod/container changed
    /// since the last sync. A no-op while nothing is selected or the cluster
    /// isn't connected yet - [`Self::new`]'s observers call this again once
    /// either changes.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let selection = match &self.pinned {
            Some(pinned) => Some(pinned.clone()),
            None => cx.try_global::<SelectedPod>().and_then(|s| s.0.clone()),
        };
        let Some(selection) = selection else {
            return;
        };
        // A selection published by a *different* context's Pods panel is not
        // this panel's to stream - see `util::shell::pod_scoped_context`'s doc
        // comment on the same bug (`1-window-context-bar` bug 1): every Pods
        // panel writes to the one app-scoped `SelectedPod`, so without this
        // check a window with two contexts open would restart this panel's
        // stream against whichever context's row was clicked last.
        if selection.context_name != self.scope.context_name {
            return;
        }
        let crate::k8s::cluster::connection::ConnectionState::Connected(client) =
            &self.connection.read(cx).state
        else {
            return;
        };
        let PodSelection {
            namespace,
            name,
            containers,
            ..
        } = selection;
        let container = containers.first().cloned().unwrap_or_default();
        let key = (namespace.clone(), name.clone(), container.clone());
        if self.current.as_ref() == Some(&key) {
            return;
        }
        self.current = Some(key);
        // A newly selected pod starts on its current logs.
        self.previous = false;
        let client = client.clone();
        self.restart(client, containers, cx);
    }

    /// Points a pod's own panel at `selection` again - reopened for its pod,
    /// perhaps naming another container, which it then switches to. Ignored
    /// by the shared panel, and for any other pod.
    pub fn pin_to(&mut self, selection: PodSelection, cx: &mut Context<Self>) {
        let Some(pinned) = &self.pinned else {
            return;
        };
        if selection.context_name != pinned.context_name
            || selection.namespace != pinned.namespace
            || selection.name != pinned.name
        {
            return;
        }
        self.pinned = Some(selection);
        self.sync(cx);
        cx.notify();
    }

    /// Test-only: the pod a pod's own panel is pinned to, and the container
    /// it shows first; `None` for the shared panel.
    #[cfg(test)]
    pub(crate) fn test_pinned(&self) -> Option<(String, Option<String>)> {
        self.pinned
            .as_ref()
            .map(|pinned| (pinned.name.clone(), pinned.containers.first().cloned()))
    }

    /// Restarts the stream on `container`, reusing the current pod/namespace -
    /// the container picker's only job, since `LogsView::select_container`
    /// already reports whether the selection actually changed.
    pub(super) fn switch_container(&mut self, container: String, cx: &mut Context<Self>) {
        let Some((namespace, name, current_container)) = self.current.clone() else {
            return;
        };
        if container == current_container {
            return;
        }
        let crate::k8s::cluster::connection::ConnectionState::Connected(client) =
            &self.connection.read(cx).state
        else {
            return;
        };
        let containers = self.view.read(cx).containers().to_vec();
        self.current = Some((namespace, name, container));
        let client = client.clone();
        self.restart(client, containers, cx);
    }

    /// `TogglePreviousLogs`: switches between the container's current logs and
    /// its previous instance's. Back on current, the stream restarts from the
    /// start, so lines written while previous logs showed are there too.
    pub(super) fn toggle_previous(&mut self, cx: &mut Context<Self>) {
        if self.current.is_none() {
            return;
        }
        let crate::k8s::cluster::connection::ConnectionState::Connected(client) =
            &self.connection.read(cx).state
        else {
            return;
        };
        let client = client.clone();
        let containers = self.view.read(cx).containers().to_vec();
        self.previous = !self.previous;
        self.restart(client, containers, cx);
        cx.notify();
    }

    /// (Re)starts the stream for the current pod and container, on the current
    /// or previous instance as `previous` says, onto a fresh view.
    fn restart(&mut self, client: kube::Client, containers: Vec<String>, cx: &mut Context<Self>) {
        let Some((namespace, pod_name, container)) = self.current.clone() else {
            return;
        };
        let mut view_model = LogsView::new(containers);
        view_model.select_container(&container);
        let view = cx.new(|_| view_model);
        cx.observe(&view, |this: &mut Self, view, cx| {
            if view.read(cx).follow_state() == FollowState::Following {
                let last = view.read(cx).lines().len().saturating_sub(1);
                this.scroll_handle
                    .scroll_to_item(last, gpui_kit::ScrollStrategy::Bottom);
            }
            cx.notify();
        })
        .detach();
        let target = LogTarget {
            namespace,
            pod_name,
            container,
            context_name: self.scope.context_name.clone(),
            previous: self.previous,
        };
        self.stream = Some(stream_container_logs(client, target, view.clone(), cx));
        self.view = view;
    }
}

impl Focusable for LogsPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for LogsPanel {}
impl EventEmitter<ScopeEvent> for LogsPanel {}

#[cfg(test)]
mod tests;
