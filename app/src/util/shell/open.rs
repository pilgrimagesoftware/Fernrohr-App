//! Opening a panel: the one path every request takes, with its dedup/focus rule, and forgetting panels the user closed.

use super::*;
use gpui_kit::component::dock::InsertTarget;

impl MainWindow {
    /// Shows `target`: a new panel in the dock if that kind is not already
    /// open in this window, otherwise a focus of the panel that is (spec 9.3).
    /// A no-op if the window has no workspace yet.
    ///
    /// Every route to a panel - double-click, the row's context menu, and the
    /// `nav.show_pods` / `nav.show_logs` commands - lands here, which is what
    /// makes 8.3's "same panel" and 9.2's "equivalent to double-click" hold by
    /// construction rather than by three call sites agreeing.
    ///
    /// Panels are built against the window's existing `context_name`, so
    /// opening one never reconnects.
    pub(super) fn open_target(
        &mut self,
        target: NavTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_target_with_view(target, None, window, cx);
    }

    /// `open_target` for a request that also says which view the panel should
    /// be showing. `initial_view` is `None` for every panel that has one view,
    /// and for a pod's detail panel opened by describing it.
    ///
    /// It applies to an already-open panel as well as a new one: focusing an
    /// existing pod panel and switching it to the requested view is what makes
    /// `y` work on a pod whose detail is already up, which is the common case
    /// rather than the exception.
    pub(super) fn open_target_with_view(
        &mut self,
        target: NavTarget,
        initial_view: Option<DetailView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_target_in(target, initial_view, None, Vec::new(), window, cx);
    }

    /// `open_target_with_view` with the scope spelled out: `context_name` is
    /// the cluster context to open the panel against (`None` for the window's
    /// own choice, as every menu, palette and row request makes), and
    /// `namespaces` the namespace scope to give it.
    ///
    /// A followed reference (`resource-links` 3.1) names its context
    /// explicitly - the context of the panel it was shown in, which in a
    /// multi-context window need not be the active one. A context this window
    /// doesn't hold is refused rather than substituted, for the same reason
    /// `pod_scoped_context` refuses a foreign selection.
    pub(crate) fn open_target_in(
        &mut self,
        target: NavTarget,
        initial_view: Option<DetailView>,
        context_name: Option<String>,
        namespaces: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace {
            dock_area,
            contexts,
            active,
            resource_panel,
            open_panels,
            nav,
            ..
        } = &mut self.mode
        else {
            return;
        };
        let connection_count = contexts.len();
        let context_name = match context_name {
            Some(named) if contexts.contains(&named) => named,
            Some(named) => {
                log::warn!(
                    "not opening {target:?}: context {named:?} is not held by this window \
                     ({contexts:?})"
                );
                return;
            }
            None => match pod_scoped_context(&target, contexts.as_slice(), *active, cx) {
                Some(context_name) => context_name,
                None => return,
            },
        };
        // Set only when a panel was actually built, so the subscription below
        // is not made for a panel the dock already had.
        let mut watch_scope = None;
        let scope = PanelScope {
            connection_count,
            ..PanelScope::new(target.clone(), context_name)
        }
        .scoped_to(namespaces);
        let key = PanelKey::from(&scope);
        let id = match open_panels.iter().find(|open| open.key == key) {
            Some(open) => {
                let id = open.id;
                if let (Some(OpenedPanel::PodDetail(panel)), Some(view)) =
                    (open.panel.as_ref(), initial_view)
                {
                    panel.update(cx, |panel, cx| panel.set_view(view, cx));
                }
                dock_area.update(cx, |area, cx| area.select_panel(id, window, cx));
                id
            }
            None => {
                // A panel opened from another one - a double-clicked row, a
                // followed link - joins the group it was opened from, rather
                // than the dock's first group: a Pods list in a lower split
                // opens its pods down there beside it.
                let source = crate::ui::panel::focus::focused_group(dock_area.read(cx), window, cx);
                let (id, opened) = dock_area.update(cx, |area, cx| {
                    let (id, opened) = nav::add_panel(area, &scope, initial_view, window, cx);
                    if let Some(node) = source {
                        let target = InsertTarget::Tabs {
                            node,
                            ix: None,
                            activate: true,
                        };
                        area.move_panel(id, target, window, cx);
                    }
                    (id, opened)
                });
                open_panels.push(OpenPanel {
                    key,
                    id,
                    panel: Some(opened.clone()),
                });
                watch_scope = Some(opened);
                id
            }
        };
        // Whichever arm ran, the panel asked for takes keyboard focus: neither
        // `select_panel` nor `add_panel_view` moves it, so without this a panel
        // opened from the keyboard needed a click before its own keys worked.
        // Only user requests come through here - a restored layout is loaded by
        // `DockArea::load`, so relaunching doesn't hop focus panel by panel.
        if let Some(panel) = dock_area.read(cx).panel(id) {
            window.focus(&panel.focus_handle(cx), cx);
        }
        **nav = target;
        let showing = (**nav).clone();
        resource_panel.update(cx, |panel, cx| panel.set_selected(Some(showing), cx));
        if let Some(opened) = watch_scope {
            self.watch_scope_changes(opened, window, cx);
        }
        cx.notify();
    }

    /// Drops bookkeeping for panels the user closed, so re-selecting one later
    /// opens a fresh panel instead of focusing a dock id that no longer exists.
    pub(super) fn forget_closed_panels(&mut self, dock_area: &Entity<DockArea>, cx: &mut App) {
        let WindowMode::Workspace { open_panels, .. } = &mut self.mode else {
            return;
        };
        let Some(tree) = dock_area.read(cx).layout(DockPlacement::Center) else {
            return;
        };
        let held: Vec<PanelId> = tree.panels().collect();
        open_panels.retain(|open| held.contains(&open.id));
    }
}

#[cfg(test)]
mod tests;
