//! `MainWindow`'s handlers for the app-level navigation and detail actions.

use super::*;

impl MainWindow {
    /// `resource.focus`: puts keyboard focus on this window's Resource panel.
    pub(super) fn on_action_focus_resources(
        &mut self,
        _: &crate::ui::resource_panel::FocusResources,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let WindowMode::Workspace { resource_panel, .. } = &self.mode {
            resource_panel.update(cx, |panel, cx| panel.focus_list(window, cx));
        }
    }

    pub(super) fn on_action_show_pods(
        &mut self,
        _: &ShowPods,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_target(NavTarget::pods(), window, cx);
    }

    pub(super) fn on_action_show_logs(
        &mut self,
        _: &ShowLogs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_target(NavTarget::Logs, window, cx);
    }

    /// Section 3.2: opens a small dialog offering Direct plus every configured
    /// tunnel for this window's connected context, writing through the same
    /// `TunnelStore::bind`/`unbind` the picker's own row selector uses. A no-op in
    /// `Picker` mode - there is no connected context to set a tunnel for yet, and
    /// the picker's own per-row selector already covers that case.
    pub(super) fn on_action_set_tunnel(
        &mut self,
        _: &SetContextTunnel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The window's active context in a workspace; in the picker, the context the
        // user selected there - so the binding can be set from the keyboard before
        // connecting, not only from a row's dropdown.
        let context_name = match &self.mode {
            WindowMode::Workspace {
                contexts, active, ..
            } => contexts[*active].clone(),
            WindowMode::Picker(picker) => match picker.read(cx).selected_context() {
                Some(context_name) => context_name,
                None => return,
            },
        };
        open_tunnel_dialog(context_name, window, cx);
    }

    /// Opens the selected pod's detail panel on the field list. Emitted by a
    /// Pods panel's `DescribePod` handler and by its row context menu's
    /// "Open".
    pub(super) fn on_action_show_pod_detail(
        &mut self,
        _: &ShowPodDetail,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_pod_detail(DetailView::Structured, window, cx);
    }

    /// Opens the selected pod's detail panel *on the YAML*. The same panel as
    /// `ShowPodDetail` opens - same target, same dock slot, same dedup - just
    /// landing on the other view, because `y` is bound to "the YAML, now" and
    /// would be pointless if it only focused a panel showing fields.
    pub(super) fn on_action_show_pod_detail_yaml(
        &mut self,
        _: &ShowPodDetailYaml,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_pod_detail(DetailView::Yaml, window, cx);
    }

    /// The single entry point for "show me this pod's detail". The pod itself
    /// travels in the app-scoped `SelectedPod` global, the same one `ShowLogs`
    /// reads, rather than in the action - `gpui_kit::actions!` generates
    /// unit-only structs, so a `ShowPodYaml` action cannot carry the pod or
    /// the view alongside it.
    pub(super) fn open_pod_detail(
        &mut self,
        view: DetailView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = cx
            .try_global::<SelectedPod>()
            .and_then(|selected| selected.0.clone())
        else {
            return;
        };
        self.open_target_with_view(
            NavTarget::pod(selection.namespace, selection.name),
            Some(view),
            window,
            cx,
        );
    }
}

#[cfg(test)]
mod tests;
