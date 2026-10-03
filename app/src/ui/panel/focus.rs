//! Keyboard focus movement between a window's panels: which panels focus can
//! step to, in what order, and the two commands that step it.
//!
//! The dock has no such movement of its own - its only actions are zoom and
//! close, and nothing in it moves keyboard focus - so this module builds the
//! order from the dock's public layout. It owns the order and the stepping; the
//! window (`util::shell::MainWindow`) owns the panels, so it runs the commands.
//! Directional (left/right/up/down) movement is out of scope: the dock exposes
//! no per-group bounds to base it on.
//!
//! Tab and Shift-Tab are the other half: they move between the controls *inside*
//! the focused panel and wrap there, never into another panel - crossing panels
//! is these commands' job. `Root` walks the window's tab stops, so on its own a
//! Tab past a panel's last control landed in the next panel. Each panel's root is
//! therefore a gpui-base `focus_trap` over the panel's handle, which `Root`
//! cycles within; the handle is itself a tab stop ([`panel_focus_handle`]), so
//! a trap is never empty. And a table took Tab for column selection, which our
//! tables never use - so focus that tabbed into one stayed there; [`tab_bindings`]
//! hands Tab in a table back to `Root`.

use crate::command::{Command, CommandRegistry, MenuSlot, NavigateGroup};
use gpui_kit::component::dock::{DockArea, DockPlacement, NodeId, PaneRef, PanelId};
use gpui_kit::{App, Entity, FocusHandle, Window, actions};

actions!(panel_focus, [FocusNextPanel, FocusPreviousPanel]);

/// A panel's own focus handle: a tab stop, so the panel's Tab trap always holds
/// one even when nothing inside it takes focus.
pub(crate) fn panel_focus_handle(cx: &mut App) -> FocusHandle {
    cx.focus_handle().tab_stop(true)
}

/// Tab and Shift-Tab inside a table, bound to `Root`'s own Tab actions - which
/// honour the panel's trap - rather than the table's column selection. Bound
/// after gpui-kit's, so they win in the table's context. `Root` doesn't export
/// its actions, so they're built by their registered names; a rename upstream
/// leaves Tab in tables as it was, logged, and fails the traversal tests.
pub fn tab_bindings(cx: &App) -> Vec<gpui_kit::KeyBinding> {
    let context = Some(std::rc::Rc::new(
        gpui_kit::KeyBindingContextPredicate::parse("DataTable").expect("a valid context"),
    ));
    [("tab", "root::Tab"), ("shift-tab", "root::TabPrev")]
        .into_iter()
        .filter_map(|(keys, name)| {
            let action = cx
                .build_action(name, None)
                .inspect_err(|error| log::warn!("no {name} action to bind {keys} to: {error}"))
                .ok()?;
            gpui_kit::KeyBinding::load(
                keys,
                action,
                context.clone(),
                false,
                None,
                cx.keyboard_mapper().as_ref(),
            )
            .ok()
        })
        .collect()
}

const FOCUS_NEXT_COMMAND_ID: &str = "panel.focus_next";
const FOCUS_PREVIOUS_COMMAND_ID: &str = "panel.focus_previous";
/// `cmd-]` / `cmd-[`, `cmd` being gpui's platform key like every other default
/// here. On macOS gpui-component's text input binds the same keys to
/// Indent/Outdent, but a single-line input registers no handler for them, so
/// the key falls through to these; elsewhere the input uses `ctrl-]` / `ctrl-[`.
pub(crate) const FOCUS_NEXT_DEFAULT_BINDING: &str = "cmd-]";
pub(crate) const FOCUS_PREVIOUS_DEFAULT_BINDING: &str = "cmd-[";

/// The dock regions in the order focus visits them: left to right, then the
/// bottom strip - how the window reads.
const REGION_ORDER: [DockPlacement; 4] = [
    DockPlacement::Left,
    DockPlacement::Center,
    DockPlacement::Right,
    DockPlacement::Bottom,
];

/// Which way a step goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Next,
    Previous,
}

/// Registers Focus Next / Previous Panel. Global rather than gated to a panel's
/// context: they are the way *between* panels, so they must work from any of
/// them - the same shape as `resource.focus`.
pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    for (id, title, default_binding, action) in [
        (
            FOCUS_NEXT_COMMAND_ID,
            "Focus Next Panel",
            FOCUS_NEXT_DEFAULT_BINDING,
            Box::new(FocusNextPanel) as Box<dyn gpui_kit::Action>,
        ),
        (
            FOCUS_PREVIOUS_COMMAND_ID,
            "Focus Previous Panel",
            FOCUS_PREVIOUS_DEFAULT_BINDING,
            Box::new(FocusPreviousPanel),
        ),
    ] {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: None,
            action,
            menu: Some(MenuSlot::Navigate(NavigateGroup::Panels)),
        });
    }
}

/// The stop after `current` in `direction`, out of `len` stops, wrapping at
/// both ends. With nothing focused (`current` is `None`), Next starts at the
/// first stop and Previous at the last. `None` only when there are no stops.
pub fn step(len: usize, current: Option<usize>, direction: Direction) -> Option<usize> {
    if len == 0 {
        return None;
    }
    Some(match (current, direction) {
        (None, Direction::Next) => 0,
        (None, Direction::Previous) => len - 1,
        (Some(current), Direction::Next) => (current + 1) % len,
        (Some(current), Direction::Previous) => (current + len - 1) % len,
    })
}

/// The dock's panels focus can land on, in visiting order: for each region in
/// [`REGION_ORDER`] that is on screen, each tab group's displayed panel in
/// pre-order (on-screen order within a split). A tab behind another in its
/// group is not on screen, so it is not a stop; neither is a panel that says it
/// isn't `visible`. While a group is zoomed it is all the area draws, so its
/// displayed panel is the only stop.
pub fn dock_stops(area: &DockArea, cx: &App) -> Vec<PanelId> {
    let mut stops = Vec::new();
    let zoomed = area.zoomed_group();
    for placement in REGION_ORDER {
        // Center is always drawn; a side dock only while open. A collapsed
        // dock still lists its panels in its tree, so this check is what keeps
        // focus off panels the user can't see.
        if placement != DockPlacement::Center && !area.is_dock_open(placement) {
            continue;
        }
        let Some(tree) = area.layout(placement) else {
            continue;
        };
        tree.root().walk(&mut |node| {
            if zoomed.is_some_and(|zoomed| zoomed != node.id()) {
                return;
            }
            if let PaneRef::Tabs { panels, active_ix } = node.kind()
                && let Some(&panel) = panels.get(active_ix)
                && area.panel(panel).is_some_and(|view| view.visible(cx))
            {
                stops.push(panel);
            }
        });
    }
    stops
}

/// The tab group holding the displayed dock panel that has keyboard focus
/// (anywhere inside it, as [`move_focus`] counts it), or `None` when focus is
/// outside the dock - the Resource panel, or nothing.
///
/// This is where a request made *from* a panel came from: a double-clicked row
/// or a followed link, whose new panel belongs in the same group.
pub fn focused_group(area: &DockArea, window: &Window, cx: &App) -> Option<NodeId> {
    let focused = dock_stops(area, cx).into_iter().find(|&panel| {
        area.panel(panel)
            .is_some_and(|view| view.focus_handle(cx).contains_focused(window, cx))
    })?;
    REGION_ORDER
        .iter()
        .find_map(|&placement| area.layout(placement)?.find_panel_node(focused))
}

/// Moves keyboard focus one stop in `direction`: the Resource panel (`resource`,
/// the first stop, when it's drawn - `None` while collapsed) then [`dock_stops`]. The current stop is the one focus is
/// anywhere inside (`contains_focused`, the same test the tab underline uses),
/// so a focused table row or filter field counts as its panel. Stops are
/// rebuilt from the live layout every time, so a panel closed since the last
/// step simply isn't one.
pub fn move_focus(
    resource: Option<FocusHandle>,
    dock_area: &Entity<DockArea>,
    direction: Direction,
    window: &mut Window,
    cx: &mut App,
) {
    let area = dock_area.read(cx);
    let stops: Vec<FocusHandle> = resource
        .into_iter()
        .chain(
            dock_stops(area, cx)
                .into_iter()
                .filter_map(|panel| area.panel(panel).map(|view| view.focus_handle(cx))),
        )
        .collect();
    let current = stops
        .iter()
        .position(|stop| stop.contains_focused(window, cx));
    if let Some(next) = step(stops.len(), current, direction) {
        window.focus(&stops[next], cx);
    }
}

#[cfg(test)]
mod tests;
