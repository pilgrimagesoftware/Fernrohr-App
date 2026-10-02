//! The cluster contexts a window holds: keeping its children in sync, switching, adding and disconnecting them, and falling back to the picker.

use super::*;

impl MainWindow {
    /// `context.add`: opens the status bar's add popover.
    pub(super) fn on_action_add_context(
        &mut self,
        _: &crate::ui::status_bar::AddContext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Deferred out of this update: the bar's dialogs read `MainWindow`.
        if let WindowMode::Workspace { status_bar, .. } = &self.mode {
            let status_bar = status_bar.clone();
            window.defer(cx, move |window, cx| {
                status_bar.update(cx, |bar, cx| bar.open_add_dialog(window, cx));
            });
        }
    }

    /// `context.disconnect`: the active context's Disconnect, with its confirmation.
    pub(super) fn on_action_disconnect_active_context(
        &mut self,
        _: &crate::ui::status_bar::DisconnectActiveContext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Deferred out of this update: the confirmation reads `MainWindow`.
        if let WindowMode::Workspace { status_bar, .. } = &self.mode {
            let status_bar = status_bar.clone();
            window.defer(cx, move |window, cx| {
                status_bar.update(cx, |bar, cx| {
                    if let Some(context_name) = bar.active_context() {
                        bar.open_disconnect_dialog(context_name, window, cx);
                    }
                });
            });
        }
    }

    /// Pushes `contexts`/`active` to the Resource panel and the status bar - the one
    /// place that updates both, so `add_context`, `disconnect_context`, and
    /// `set_active_context` cannot update one and forget the other (see
    /// `WindowMode::Workspace::status_bar`'s doc comment), plus the window's title.
    /// A no-op in `Picker` mode.
    pub(super) fn sync_context_children(&mut self, cx: &mut Context<Self>) {
        let WindowMode::Workspace {
            contexts,
            active,
            resource_panel,
            status_bar,
            ..
        } = &self.mode
        else {
            return;
        };
        let contexts_snapshot = contexts.clone();
        let active_index = *active;
        let Some(active_context) = contexts_snapshot.get(active_index).cloned() else {
            return;
        };
        let resource_panel = resource_panel.clone();
        let status_bar = status_bar.clone();
        let main_window = cx.weak_entity();
        // Deferred: a capsule click (`ui/status_bar/capsule.rs`'s
        // `StatusBarView::activate`) and the Resource panel's own cluster dropdown
        // (`ResourcePanel::cluster_dropdown`'s `cx.emit`) both reach this
        // synchronously from within that very entity's own update - updating it
        // again here, before that update returns, panics ("cannot update T while
        // it is already being updated"). `cx.defer` runs this closure once the
        // current effect cycle finishes and every lease along the way to here has
        // released, which resolves before any `Entity::update`/`WindowHandle::
        // update` call that reached `set_active_context` returns - so callers
        // still observe the synced state immediately afterward, same as before
        // this was deferred.
        cx.defer(move |cx| {
            resource_panel.update(cx, |panel, cx| {
                panel.set_active_context(active_context, contexts_snapshot.clone(), cx);
            });
            status_bar.update(cx, |bar, cx| {
                bar.set_context_names(contexts_snapshot, cx);
                bar.set_active(active_index, cx);
            });
            // After the guards above, so only a workspace with a live active
            // context is re-titled here (design.md decision 5).
            window_title::apply_deferred(&main_window, cx);
        });
        cx.notify();
    }

    /// `window-context-bar` design.md decision 4: the one place `active` is
    /// written - a chip click and the Resource panel's own cluster dropdown both
    /// land here, which is what keeps the two in sync. A no-op if `context_name`
    /// isn't one this window uses (a stale request racing a disconnect).
    pub(crate) fn set_active_context(
        &mut self,
        context_name: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace {
            contexts, active, ..
        } = &mut self.mode
        else {
            return;
        };
        let Some(index) = contexts.iter().position(|name| name == context_name) else {
            return;
        };
        *active = index;
        self.sync_context_children(cx);
    }

    /// Section 3.2: adds `context_name` to this window, makes it the active
    /// context, and opens its Pods panel. Called only after
    /// the status bar's add popover already reports
    /// `PickerEvent::Connected` for it, so by the time this runs the connection
    /// has already succeeded - a context that fails to connect never reaches
    /// this at all, which is what keeps a failed add from opening a panel.
    pub(crate) fn add_context(
        &mut self,
        context_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace { contexts, .. } = &self.mode else {
            return;
        };
        if contexts.contains(&context_name) {
            return;
        }

        let window_id = window.window_handle().window_id();
        ClusterRegistry::hold(cx, &context_name, window_id);

        let WindowMode::Workspace {
            contexts, active, ..
        } = &mut self.mode
        else {
            return;
        };
        contexts.push(context_name);
        *active = contexts.len() - 1;
        self.sync_context_children(cx);
        self.open_target(NavTarget::pods(), window, cx);
    }

    /// Section 3.3: closes every panel in this window that uses `context_name`,
    /// releases this window's hold on it, and removes its chip. Falls back to
    /// the cluster picker when that was the window's last context.
    pub(crate) fn disconnect_context(
        &mut self,
        context_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace {
            dock_area,
            open_panels,
            ..
        } = &self.mode
        else {
            return;
        };
        let dock_area = dock_area.clone();
        let closing: Vec<(PanelId, Option<OpenedPanel>)> = open_panels
            .iter()
            .filter(|open| open.key.context_name == context_name)
            .map(|open| (open.id, open.panel.clone()))
            .collect();

        for (id, opened) in closing {
            let opened = opened.or_else(|| nav::opened_panel_for(dock_area.read(cx), id, cx));
            let Some(opened) = opened else {
                continue;
            };
            dock_area.update(cx, |area, cx| match opened {
                OpenedPanel::Pods(panel) => area.remove_panel(panel, window, cx),
                OpenedPanel::ObjectList(panel) => area.remove_panel(panel, window, cx),
                OpenedPanel::Placeholder(panel) => area.remove_panel(panel, window, cx),
                OpenedPanel::Logs(panel) => area.remove_panel(panel, window, cx),
                OpenedPanel::PodDetail(panel) => area.remove_panel(panel, window, cx),
                OpenedPanel::ObjectDetail(panel) => area.remove_panel(panel, window, cx),
            });
        }

        let WindowMode::Workspace { open_panels, .. } = &mut self.mode else {
            return;
        };
        open_panels.retain(|open| open.key.context_name != context_name);

        let window_id = window.window_handle().window_id();
        ClusterRegistry::release(cx, &context_name, window_id);

        let WindowMode::Workspace {
            contexts, active, ..
        } = &self.mode
        else {
            return;
        };
        let outcome =
            context_lifecycle::contexts_after_disconnect(contexts, *active, &context_name);

        match outcome {
            Some((remaining, new_active)) => {
                let WindowMode::Workspace {
                    contexts, active, ..
                } = &mut self.mode
                else {
                    return;
                };
                *contexts = remaining;
                *active = new_active;
                self.sync_context_children(cx);
            }
            None => self.enter_picker(window, cx),
        }
    }

    pub(super) fn enter_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picker = cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx));
        watch_picker(&picker, window, cx);
        self.mode = WindowMode::Picker(picker);
        // No children to sync, so `sync_context_children` never runs here:
        // the window drops back to the plain app-name title itself.
        window_title::apply(&self.mode, window);
        cx.notify();
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod capsule_tests;
