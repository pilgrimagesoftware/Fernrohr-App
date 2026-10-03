//! Moving a whole context to one namespace scope (`warp-all-to-namespace`): every
//! namespaced list in this window that uses the context is re-scoped, and the
//! scope becomes the context's default for lists opened afterwards. The Pods
//! panel's Warp All to Namespace asks for it through [`WarpContextToNamespace`];
//! [`MainWindow::warp_context`] is the reusable part, for any caller that moves a
//! context at once (namespace sets, next).

use super::namespace_defaults::NamespaceDefaults;
use super::{MainWindow, WindowMode};
use crate::ui::nav::{self, NavTarget, OpenedPanel};
use gpui_kit::component::dock::PanelId;
use gpui_kit::*;

/// Asks the window to warp every namespaced list in `context_name` to `namespace`.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = shell, no_json)]
pub struct WarpContextToNamespace {
    pub context_name: String,
    pub namespace: String,
}

impl MainWindow {
    pub(super) fn on_action_warp_context(
        &mut self,
        action: &WarpContextToNamespace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.warp_context(&action.context_name, vec![action.namespace.clone()], cx);
    }

    /// Scopes every namespaced list in this window that uses `context_name` to
    /// `namespaces` (empty for all), and makes that the context's default for lists
    /// opened afterwards. Cluster-scoped lists, detail panels and logs keep their
    /// scope; so do other contexts' panels. Returns how many panels it moved.
    pub(crate) fn warp_context(
        &mut self,
        context_name: &str,
        namespaces: Vec<String>,
        cx: &mut Context<Self>,
    ) -> usize {
        let WindowMode::Workspace {
            open_panels,
            dock_area,
            ..
        } = &self.mode
        else {
            return 0;
        };
        let candidates: Vec<(PanelId, Option<OpenedPanel>)> = open_panels
            .iter()
            .filter(|open| open.key.context_name == context_name)
            .filter(|open| matches!(&open.key.target, NavTarget::Kind(kind) if kind.namespaced))
            .map(|open| (open.id, open.panel.clone()))
            .collect();
        let dock_area = dock_area.clone();
        let mut moved = Vec::new();
        for (id, opened) in candidates {
            // A restored panel has no typed handle here; the dock still has it.
            let Some(opened) = opened.or_else(|| nav::opened_panel_for(dock_area.read(cx), id, cx))
            else {
                continue;
            };
            if opened.set_namespaces(namespaces.clone(), cx) {
                moved.push(id);
            }
        }
        // The panels report the change too; recording it here as well covers a
        // restored panel this window never subscribed to.
        for id in &moved {
            self.rescope(*id, namespaces.clone(), cx);
        }
        NamespaceDefaults::set(cx, context_name, namespaces);
        cx.notify();
        moved.len()
    }
}

#[cfg(test)]
mod tests;
