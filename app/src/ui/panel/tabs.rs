//! Keyboard switching between the tabs of a dock group: which group is "the
//! focused tab group", which tab a command targets, and the commands.
//!
//! The dock's tab groups are private to it, so this reads the public layout
//! (`PaneRef::Tabs`) and switches with `DockArea::select_panel`. It owns the
//! choice of group and tab; the window (`util::shell::tabs`) runs the commands
//! and moves focus. `Cmd-W`'s close-a-tab path uses the same focused group, so
//! "the current tab" means one thing for switching and closing.

use super::focus;
use crate::command::{Command, CommandRegistry, MenuSlot};
use gpui_kit::component::dock::{DockArea, DockPlacement, NodeId, PaneRef, PaneTree, PanelId};
use gpui_kit::{Action, App, Window, actions};

actions!(
    panel_tabs,
    [
        NextTab,
        PreviousTab,
        SelectTab1,
        SelectTab2,
        SelectTab3,
        SelectTab4,
        SelectTab5,
        SelectTab6,
        SelectTab7,
        SelectTab8,
        SelectTab9
    ]
);

/// The tab a command asks for, relative to the group's displayed tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabTarget {
    Next,
    Previous,
    /// 1-based, as the command names it; 9 means the last tab.
    Position(usize),
}

/// A tab group: its tabs in strip order, and which one is displayed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabGroup {
    pub panels: Vec<PanelId>,
    pub active_ix: usize,
}

/// Registers Next/Previous Tab and Select Tab 1-9. Global, like the
/// `panel.*` commands: the dock has no key context of its own to scope them
/// to, and they have to work with focus in any panel.
pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    let mut register = |id, title, default_binding, action: Box<dyn Action>, menu| {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: None,
            action,
            menu,
        });
    };
    register(
        "tab.next",
        "Next Tab",
        "cmd-shift-]",
        Box::new(NextTab),
        Some(MenuSlot::Navigate),
    );
    register(
        "tab.previous",
        "Previous Tab",
        "cmd-shift-[",
        Box::new(PreviousTab),
        Some(MenuSlot::Navigate),
    );
    // In the palette, not the menu: nine near-identical items would bury it.
    let selects: [(&str, &str, &str, Box<dyn Action>); 9] = [
        (
            "tab.select_1",
            "Select Tab 1",
            "ctrl-1",
            Box::new(SelectTab1),
        ),
        (
            "tab.select_2",
            "Select Tab 2",
            "ctrl-2",
            Box::new(SelectTab2),
        ),
        (
            "tab.select_3",
            "Select Tab 3",
            "ctrl-3",
            Box::new(SelectTab3),
        ),
        (
            "tab.select_4",
            "Select Tab 4",
            "ctrl-4",
            Box::new(SelectTab4),
        ),
        (
            "tab.select_5",
            "Select Tab 5",
            "ctrl-5",
            Box::new(SelectTab5),
        ),
        (
            "tab.select_6",
            "Select Tab 6",
            "ctrl-6",
            Box::new(SelectTab6),
        ),
        (
            "tab.select_7",
            "Select Tab 7",
            "ctrl-7",
            Box::new(SelectTab7),
        ),
        (
            "tab.select_8",
            "Select Tab 8",
            "ctrl-8",
            Box::new(SelectTab8),
        ),
        (
            "tab.select_9",
            "Select Last Tab",
            "ctrl-9",
            Box::new(SelectTab9),
        ),
    ];
    for (id, title, default_binding, action) in selects {
        register(id, title, default_binding, action, None);
    }
}

/// The index `target` picks in a group of `len` tabs showing `active`. Next
/// and Previous wrap; Position 1-8 index directly and 9 is the last tab; a
/// position past the end, or an empty group, picks nothing.
pub fn target_index(len: usize, active: usize, target: TabTarget) -> Option<usize> {
    match target {
        TabTarget::Next => focus::step(len, Some(active), focus::Direction::Next),
        TabTarget::Previous => focus::step(len, Some(active), focus::Direction::Previous),
        TabTarget::Position(9) => len.checked_sub(1),
        TabTarget::Position(n) => (1..=len).contains(&n).then(|| n - 1),
    }
}

/// The focused tab group: [`focus::focused_group`] - the group focus is in -
/// or, with focus outside every group (the Resource panel), the first group
/// in panel-focus order, the one `cmd-]` would move to. The fallback is what
/// lets these commands, and `Cmd-W`, act on the tabs on screen from outside
/// the dock. `None` only for a dock with no panels on screen.
pub fn focused_group(area: &DockArea, window: &Window, cx: &App) -> Option<TabGroup> {
    let node = focus::focused_group(area, window, cx).or_else(|| {
        let first = *focus::dock_stops(area, cx).first()?;
        regions(area).find_map(|tree| tree.find_panel_node(first))
    })?;
    tabs_of(area, node)
}

fn regions(area: &DockArea) -> impl Iterator<Item = &PaneTree> {
    [
        DockPlacement::Left,
        DockPlacement::Center,
        DockPlacement::Right,
        DockPlacement::Bottom,
    ]
    .into_iter()
    .filter_map(|placement| area.layout(placement))
}

/// The panel the tab group `node` displays, if `node` is still a tab group.
pub fn active_panel_of(area: &DockArea, node: NodeId) -> Option<PanelId> {
    let group = tabs_of(area, node)?;
    group.panels.get(group.active_ix).copied()
}

/// The tab group `panel` is in, wherever in the dock.
pub fn group_of(area: &DockArea, panel: PanelId) -> Option<NodeId> {
    regions(area).find_map(|tree| tree.find_panel_node(panel))
}

/// The tabs of the group `node`, wherever in the dock it is.
fn tabs_of(area: &DockArea, node: NodeId) -> Option<TabGroup> {
    regions(area).find_map(|tree| {
        let node = tree.find_node(node)?;
        match node.kind() {
            PaneRef::Tabs { panels, active_ix } => Some(TabGroup {
                panels: panels.to_vec(),
                active_ix,
            }),
            PaneRef::Split { .. } => None,
        }
    })
}

#[cfg(test)]
mod tests;
