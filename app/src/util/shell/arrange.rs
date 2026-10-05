//! `MainWindow`'s arrange commands (`panel-move-keybindings`): split the focused
//! group, move the focused panel into the group beside it, merge the focused
//! group into the one beside it, and close the focused group - each the dock's
//! own edit (`DockArea::move_panel`), so the result is what the same drag would
//! leave, and the saved layout needs nothing new.
//!
//! Which group is beside which is [`crate::ui::panel::arrange`]'s geometry.
//! These act on the centre region's groups, the ones a user splits; with focus
//! outside them, or no group that way, they do nothing.

use super::*;
use crate::ui::panel::arrange::{
    self, Direction, MovePanelDown, MovePanelLeft, MovePanelRight, MovePanelUp, SplitGroupDown,
    SplitGroupLeft, SplitGroupRight, SplitGroupUp,
};
use crate::ui::panel::tabs;
use gpui_kit::component::dock::{InsertTarget, NodeId};

impl MainWindow {
    /// The arrange commands' listeners, for the window's root element.
    pub(super) fn with_arrange_actions(element: Div, cx: &mut Context<Self>) -> Div {
        use Direction::{Down, Left, Right, Up};
        let split = |direction| {
            move |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                this.split_group(direction, window, cx)
            }
        };
        let moved = |direction| {
            move |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                this.move_focused_panel(direction, window, cx)
            }
        };
        let (sl, sr, su, sd) = (split(Left), split(Right), split(Up), split(Down));
        let (ml, mr, mu, md) = (moved(Left), moved(Right), moved(Up), moved(Down));
        element
            .on_action(cx.listener(move |t, _: &SplitGroupLeft, w, cx| sl(t, w, cx)))
            .on_action(cx.listener(move |t, _: &SplitGroupRight, w, cx| sr(t, w, cx)))
            .on_action(cx.listener(move |t, _: &SplitGroupUp, w, cx| su(t, w, cx)))
            .on_action(cx.listener(move |t, _: &SplitGroupDown, w, cx| sd(t, w, cx)))
            .on_action(cx.listener(move |t, _: &MovePanelLeft, w, cx| ml(t, w, cx)))
            .on_action(cx.listener(move |t, _: &MovePanelRight, w, cx| mr(t, w, cx)))
            .on_action(cx.listener(move |t, _: &MovePanelUp, w, cx| mu(t, w, cx)))
            .on_action(cx.listener(move |t, _: &MovePanelDown, w, cx| md(t, w, cx)))
    }

    /// The dock, and the centre-region group focus is in - none from outside one.
    fn focused_center_group(
        &self,
        window: &Window,
        cx: &App,
    ) -> Option<(Entity<DockArea>, NodeId)> {
        let WindowMode::Workspace { dock_area, .. } = &self.mode else {
            return None;
        };
        let area = dock_area.read(cx);
        let node = crate::ui::panel::focus::focused_group(area, window, cx)?;
        area.layout(DockPlacement::Center)?.find_node(node)?;
        Some((dock_area.clone(), node))
    }

    /// The centre-region group beside `node` in `direction`.
    fn group_beside(area: &DockArea, node: NodeId, direction: Direction) -> Option<NodeId> {
        let tree = area.layout(DockPlacement::Center)?;
        arrange::adjacent(&arrange::group_rects(tree.root()), node, direction)
    }

    /// Splits the focused group: a new group in `direction` holding a second
    /// panel over the focused panel's target, focused.
    fn split_group(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        let Some((dock_area, node)) = self.focused_center_group(window, cx) else {
            return;
        };
        let Some(panel) = tabs::active_panel_of(dock_area.read(cx), node) else {
            return;
        };
        let WindowMode::Workspace {
            open_panels,
            contexts,
            ..
        } = &mut self.mode
        else {
            return;
        };
        let Some(key) = open_panels
            .iter()
            .find(|open| open.id == panel)
            .map(|open| open.key.clone())
        else {
            return;
        };
        // A copy, not a re-selection: built straight from the key, past
        // `open_target_in`'s one-panel-per-key lookup.
        let scope = PanelScope {
            connection_count: contexts.len(),
            ..PanelScope::new(key.target.clone(), key.context_name.clone())
        }
        .scoped_to(key.namespaces.clone());
        let (id, opened) = dock_area.update(cx, |area, cx| {
            let (id, opened) = nav::add_panel(area, &scope, None, window, cx);
            let beside = InsertTarget::Split {
                node,
                placement: direction.placement(),
                size: None,
            };
            area.move_panel(id, beside, window, cx);
            (id, opened)
        });
        open_panels.push(OpenPanel {
            key,
            id,
            panel: Some(opened.clone()),
            group: None,
            _focus_watch: Self::watch_panel_focus(&dock_area, id, window, cx),
        });
        self.watch_scope_changes(opened, window, cx);
        Self::focus_panel(&dock_area, id, window, cx);
        cx.notify();
    }

    /// Moves the focused panel into the group beside its own, in `direction`.
    fn move_focused_panel(
        &mut self,
        direction: Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((dock_area, node)) = self.focused_center_group(window, cx) else {
            return;
        };
        let area = dock_area.read(cx);
        let (Some(panel), Some(beside)) = (
            tabs::active_panel_of(area, node),
            Self::group_beside(area, node, direction),
        ) else {
            return;
        };
        dock_area.update(cx, |area, cx| {
            let into = InsertTarget::Tabs {
                node: beside,
                ix: None,
                activate: true,
            };
            area.move_panel(panel, into, window, cx);
        });
        Self::focus_panel(&dock_area, panel, window, cx);
        cx.notify();
    }

    fn focus_panel(
        dock_area: &Entity<DockArea>,
        panel: PanelId,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(view) = dock_area.read(cx).panel(panel) {
            window.focus(&view.focus_handle(cx), cx);
        }
    }
}

#[cfg(test)]
mod tests;
