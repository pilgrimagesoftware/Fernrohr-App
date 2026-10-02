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
            let scope = PanelScope::new(NavTarget::Logs, context_name).scoped_to(namespaces);
            Ok(panel_handle(cx.new(|cx| LogsPanel::new(scope, cx))))
        })
    });
}

/// A dock panel streaming the container logs of whichever pod was last
/// clicked in a Pods panel (see [`SelectedPod`]).
pub struct LogsPanel {
    pub(super) scope: PanelScope,
    connection: Entity<crate::k8s::cluster::connection::ClusterConnection>,
    pub(super) view: Entity<LogsView>,
    stream: Option<Task<()>>,
    pub(super) current: Option<(String, String, String)>,
    pub(super) scroll_handle: UniformListScrollHandle,
    pub(super) focus_handle: FocusHandle,
}

impl LogsPanel {
    pub fn new(scope: PanelScope, cx: &mut Context<Self>) -> Self {
        use crate::k8s::cluster::session::ClusterRegistry;

        let connection = ClusterRegistry::connection(cx, &scope.context_name);
        cx.observe(&connection, |this: &mut Self, _, cx| this.sync(cx))
            .detach();
        cx.observe_global::<SelectedPod>(|this: &mut Self, cx| this.sync(cx))
            .detach();

        let mut this = Self {
            scope,
            connection,
            view: cx.new(|_| LogsView::new(vec![String::new()])),
            stream: None,
            current: None,
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
        let Some(selection) = cx.try_global::<SelectedPod>().and_then(|s| s.0.clone()) else {
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
            context_name,
        } = selection;
        let container = containers.first().cloned().unwrap_or_default();
        let key = (namespace.clone(), name.clone(), container.clone());
        if self.current.as_ref() == Some(&key) {
            return;
        }
        self.current = Some(key);

        let client = client.clone();
        let view = cx.new(|_| LogsView::new(containers));
        cx.observe(&view, |this: &mut Self, view, cx| {
            if view.read(cx).follow_state() == FollowState::Following {
                let last = view.read(cx).lines().len().saturating_sub(1);
                this.scroll_handle
                    .scroll_to_item(last, gpui_kit::ScrollStrategy::Bottom);
            }
            cx.notify();
        })
        .detach();
        self.stream = Some(stream_container_logs(
            client,
            namespace,
            name,
            container,
            context_name,
            view.clone(),
            cx,
        ));
        self.view = view;
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
        self.current = Some((namespace.clone(), name.clone(), container.clone()));

        let client = client.clone();
        let view = cx.new(|_| LogsView::new(containers));
        cx.observe(&view, |this: &mut Self, view, cx| {
            if view.read(cx).follow_state() == FollowState::Following {
                let last = view.read(cx).lines().len().saturating_sub(1);
                this.scroll_handle
                    .scroll_to_item(last, gpui_kit::ScrollStrategy::Bottom);
            }
            cx.notify();
        })
        .detach();
        self.stream = Some(stream_container_logs(
            client,
            namespace,
            name,
            container,
            self.scope.context_name.clone(),
            view.clone(),
            cx,
        ));
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
