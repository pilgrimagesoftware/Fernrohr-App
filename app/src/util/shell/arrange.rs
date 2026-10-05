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
    Direction, SplitGroupDown, SplitGroupLeft, SplitGroupRight, SplitGroupUp,
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
        let (sl, sr, su, sd) = (split(Left), split(Right), split(Up), split(Down));
        element
            .on_action(cx.listener(move |t, _: &SplitGroupLeft, w, cx| sl(t, w, cx)))
            .on_action(cx.listener(move |t, _: &SplitGroupRight, w, cx| sr(t, w, cx)))
            .on_action(cx.listener(move |t, _: &SplitGroupUp, w, cx| su(t, w, cx)))
            .on_action(cx.listener(move |t, _: &SplitGroupDown, w, cx| sd(t, w, cx)))
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
