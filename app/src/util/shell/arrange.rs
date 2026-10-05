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
    self, ClosePanelGroup, Direction, MergeGroupDown, MergeGroupLeft, MergeGroupRight,
    MergeGroupUp, MovePanelDown, MovePanelLeft, MovePanelRight, MovePanelUp, SplitGroupDown,
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
        let merge = |direction| {
            move |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                this.merge_group(direction, window, cx)
            }
        };
        let (sl, sr, su, sd) = (split(Left), split(Right), split(Up), split(Down));
        let (ml, mr, mu, md) = (moved(Left), moved(Right), moved(Up), moved(Down));
        let (gl, gr, gu, gd) = (merge(Left), merge(Right), merge(Up), merge(Down));
        element
            .on_action(cx.listener(move |t, _: &SplitGroupLeft, w, cx| sl(t, w, cx)))
            .on_action(cx.listener(move |t, _: &SplitGroupRight, w, cx| sr(t, w, cx)))
            .on_action(cx.listener(move |t, _: &SplitGroupUp, w, cx| su(t, w, cx)))
            .on_action(cx.listener(move |t, _: &SplitGroupDown, w, cx| sd(t, w, cx)))
            .on_action(cx.listener(move |t, _: &MovePanelLeft, w, cx| ml(t, w, cx)))
            .on_action(cx.listener(move |t, _: &MovePanelRight, w, cx| mr(t, w, cx)))
            .on_action(cx.listener(move |t, _: &MovePanelUp, w, cx| mu(t, w, cx)))
            .on_action(cx.listener(move |t, _: &MovePanelDown, w, cx| md(t, w, cx)))
            .on_action(cx.listener(move |t, _: &MergeGroupLeft, w, cx| gl(t, w, cx)))
            .on_action(cx.listener(move |t, _: &MergeGroupRight, w, cx| gr(t, w, cx)))
            .on_action(cx.listener(move |t, _: &MergeGroupUp, w, cx| gu(t, w, cx)))
            .on_action(cx.listener(move |t, _: &MergeGroupDown, w, cx| gd(t, w, cx)))
            .on_action(
                cx.listener(|this, _: &ClosePanelGroup, window, cx| this.close_group(window, cx)),
            )
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

    /// Moves every panel of the focused group, in order, into the group beside
    /// it in `direction`; the emptied group goes. The focused panel keeps focus.
    fn merge_group(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        let Some((dock_area, node)) = self.focused_center_group(window, cx) else {
            return;
        };
        let area = dock_area.read(cx);
        let (Some(group), Some(beside)) = (
            tabs::tabs_of(area, node),
            Self::group_beside(area, node, direction),
        ) else {
            return;
        };
        let focused = group.panels.get(group.active_ix).copied();
        dock_area.update(cx, |area, cx| {
            for &panel in &group.panels {
                let into = InsertTarget::Tabs {
                    node: beside,
                    ix: None,
                    activate: Some(panel) == focused,
                };
                area.move_panel(panel, into, window, cx);
            }
        });
        if let Some(panel) = focused {
            Self::focus_panel(&dock_area, panel, window, cx);
        }
        cx.notify();
    }

    /// Closes every panel of the focused group - asking first, once, when any
    /// would lose something by closing (a running shell, an unsaved edit).
    fn close_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((dock_area, node)) = self.focused_center_group(window, cx) else {
            return;
        };
        let Some(group) = tabs::tabs_of(dock_area.read(cx), node) else {
            return;
        };
        let warnings: Vec<String> = group
            .panels
            .iter()
            .filter_map(|&panel| close_warning(dock_area.read(cx), panel, cx))
            .collect();
        if warnings.is_empty() {
            close_panels(&dock_area, &group.panels, window, cx);
            return;
        }
        let panels = group.panels.clone();
        let confirmation = super::tabs::CloseConfirmation {
            title: "Close Group?",
            body: format!(
                "Closing this group closes {} panels. {}",
                panels.len(),
                warnings.join(" ")
            ),
            confirm: "Close Group",
            id_prefix: "close-group",
        };
        super::tabs::open_close_confirmation(
            confirmation,
            move |window, cx| close_panels(&dock_area, &panels, window, cx),
            window,
            cx,
        );
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

/// What closing `panel` would cost, from the panels that can say.
fn close_warning(area: &DockArea, panel: PanelId, cx: &App) -> Option<String> {
    match nav::opened_panel_for(area, panel, cx)? {
        OpenedPanel::Exec(panel) => panel.read(cx).close_warning(),
        OpenedPanel::ObjectDetail(panel) => panel.read(cx).close_warning(),
        OpenedPanel::Pods(_)
        | OpenedPanel::ObjectList(_)
        | OpenedPanel::Events(_)
        | OpenedPanel::Placeholder(_)
        | OpenedPanel::Logs(_)
        | OpenedPanel::PodDetail(_) => None,
    }
}

/// Closes each of `panels` through the dock's own close, as its tab's close
/// button does.
fn close_panels(
    dock_area: &Entity<DockArea>,
    panels: &[PanelId],
    window: &mut Window,
    cx: &mut App,
) {
    for &panel in panels {
        let Some(opened) = nav::opened_panel_for(dock_area.read(cx), panel, cx) else {
            continue;
        };
        dock_area.update(cx, |area, cx| match opened {
            OpenedPanel::Pods(panel) => area.remove_panel(panel, window, cx),
            OpenedPanel::ObjectList(panel) => area.remove_panel(panel, window, cx),
            OpenedPanel::Events(panel) => area.remove_panel(panel, window, cx),
            OpenedPanel::Placeholder(panel) => area.remove_panel(panel, window, cx),
            OpenedPanel::Logs(panel) => area.remove_panel(panel, window, cx),
            OpenedPanel::PodDetail(panel) => area.remove_panel(panel, window, cx),
            OpenedPanel::ObjectDetail(panel) => area.remove_panel(panel, window, cx),
            OpenedPanel::Exec(panel) => area.remove_panel(panel, window, cx),
        });
    }
}

#[cfg(test)]
mod tests;
