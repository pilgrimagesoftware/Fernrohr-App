//! What the window asks of an [`OpenedPanel`] without naming its type: its
//! namespaces, its pin, its dock id and its focus.

use super::OpenedPanel;
use gpui_kit::component::dock::PanelId;
use gpui_kit::*;

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
            OpenedPanel::HelmReleases(panel) => {
                panel.update(cx, |p, cx| p.set_namespaces(namespaces, cx))
            }
            OpenedPanel::Placeholder(panel) => {
                panel.update(cx, |p, cx| p.set_namespaces(namespaces, cx))
            }
            // Only a typed label panel's namespaces are its own to change.
            OpenedPanel::Logs(panel) => {
                return panel.update(cx, |p, cx| p.set_namespaces(namespaces, cx));
            }
            OpenedPanel::PodDetail(_)
            | OpenedPanel::ObjectDetail(_)
            | OpenedPanel::Exec(_)
            | OpenedPanel::HelmRelease(_) => {
                return false;
            }
        }
        true
    }

    /// Whether the panel's tab is pinned (`panel-tab-menu`).
    pub fn pinned(&self, cx: &App) -> bool {
        use crate::ui::panel_title::ScopedPanel as _;
        match self {
            OpenedPanel::Pods(panel) => panel.read(cx).scope().pinned,
            OpenedPanel::ObjectList(panel) => panel.read(cx).scope().pinned,
            OpenedPanel::Events(panel) => panel.read(cx).scope().pinned,
            OpenedPanel::HelmReleases(panel) => panel.read(cx).scope().pinned,
            OpenedPanel::HelmRelease(panel) => panel.read(cx).scope().pinned,
            OpenedPanel::Placeholder(panel) => panel.read(cx).scope().pinned,
            OpenedPanel::Logs(panel) => panel.read(cx).scope().pinned,
            OpenedPanel::PodDetail(panel) => panel.read(cx).scope().pinned,
            OpenedPanel::ObjectDetail(panel) => panel.read(cx).scope().pinned,
            OpenedPanel::Exec(panel) => panel.read(cx).scope().pinned,
        }
    }

    /// Pins or unpins the panel's tab; it redraws, so its title shows which.
    pub fn set_pinned(&self, pinned: bool, cx: &mut App) {
        fn set<P: crate::ui::panel_title::ScopedPanel + 'static>(
            panel: &Entity<P>,
            pinned: bool,
            cx: &mut App,
        ) {
            panel.update(cx, |panel, cx| {
                panel.scope_mut().pinned = pinned;
                cx.notify();
            });
        }
        match self {
            OpenedPanel::Pods(panel) => set(panel, pinned, cx),
            OpenedPanel::ObjectList(panel) => set(panel, pinned, cx),
            OpenedPanel::Events(panel) => set(panel, pinned, cx),
            OpenedPanel::HelmReleases(panel) => set(panel, pinned, cx),
            OpenedPanel::HelmRelease(panel) => set(panel, pinned, cx),
            OpenedPanel::Placeholder(panel) => set(panel, pinned, cx),
            OpenedPanel::Logs(panel) => set(panel, pinned, cx),
            OpenedPanel::PodDetail(panel) => set(panel, pinned, cx),
            OpenedPanel::ObjectDetail(panel) => set(panel, pinned, cx),
            OpenedPanel::Exec(panel) => set(panel, pinned, cx),
        }
    }

    /// The dock id of the panel just built, without naming its type - what the
    /// window files it under and what `rescope` needs to find it again.
    pub fn panel_id(&self) -> PanelId {
        match self {
            OpenedPanel::Pods(panel) => PanelId::from(panel.entity_id()),
            OpenedPanel::ObjectList(panel) => PanelId::from(panel.entity_id()),
            OpenedPanel::Events(panel) => PanelId::from(panel.entity_id()),
            OpenedPanel::HelmReleases(panel) => PanelId::from(panel.entity_id()),
            OpenedPanel::HelmRelease(panel) => PanelId::from(panel.entity_id()),
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
            OpenedPanel::HelmReleases(panel) => panel.read(cx).focus_handle(cx),
            OpenedPanel::HelmRelease(panel) => panel.read(cx).focus_handle(cx),
            OpenedPanel::Placeholder(panel) => panel.read(cx).focus_handle(cx),
            OpenedPanel::Logs(panel) => panel.read(cx).focus_handle(cx),
            OpenedPanel::PodDetail(panel) => panel.read(cx).focus_handle(cx),
            OpenedPanel::ObjectDetail(panel) => panel.read(cx).focus_handle(cx),
            OpenedPanel::Exec(panel) => panel.read(cx).focus_handle(cx),
        }
    }
}
