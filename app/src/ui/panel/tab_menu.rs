//! A panel tab's context menu (`panel-tab-menu`): the tab-scoped commands it
//! offers, what decides which are enabled, and where a tab's title was last
//! drawn, so the keyboard can open the menu at the focused tab.
//!
//! Every item is a registered command acting on the focused tab - in the
//! palette, rebindable, and the same action a key runs. The menu targets the
//! tab it was opened for: choosing an item focuses that tab, then dispatches
//! the item's command, so the command resolves its tab and group afresh at
//! that moment rather than from what the menu saw when it opened. Drawing the
//! menu and running the commands is `util::shell::tab_menu`'s; this module is
//! the model the window and the tab title share.

use crate::command::{Command, CommandRegistry, MenuSlot, ViewGroup};
use gpui_kit::*;
use std::collections::HashMap;

actions!(
    panel_tab_menu,
    [
        CloseTab,
        CloseOtherTabs,
        CloseTabsToRight,
        MoveTabToNewWindow,
        PinTab,
        UnpinTab,
        OpenTabMenu
    ]
);

/// Opens the tab menu at `position` for the panel `panel` - what a right-click
/// on a tab dispatches. Names its panel, since a right-click can target a tab
/// that isn't the focused one. Not a palette command (the palette has
/// [`OpenTabMenu`], for the focused tab).
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = panel_tab_menu, no_json)]
pub struct OpenTabMenuAt {
    pub panel: EntityId,
    pub position: Point<Pixels>,
}

/// [`OpenTabMenu`]'s default key: `cmd-k` then `m`, beside the dock's other
/// `cmd-k` chords (`ui::panel::arrange`).
pub const OPEN_TAB_MENU_KEY: &str = "cmd-k m";

/// One item of the tab menu, in menu order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabMenuItem {
    CloseTab,
    CloseOthers,
    CloseToRight,
    CloseAll,
    MoveToNewWindow,
    Pin,
    Unpin,
}

impl TabMenuItem {
    /// What the menu shows: the item's command's title, as the palette shows
    /// it - except Close All Tabs, which runs the dock's Close Group
    /// (`panels.close_group`), named here for what it does to this tab's group.
    pub fn label(self) -> &'static str {
        match self {
            Self::CloseTab => CLOSE_TAB_TITLE,
            Self::CloseOthers => CLOSE_OTHERS_TITLE,
            Self::CloseToRight => CLOSE_RIGHT_TITLE,
            Self::CloseAll => "Close All Tabs",
            Self::MoveToNewWindow => MOVE_TITLE,
            Self::Pin => PIN_TITLE,
            Self::Unpin => UNPIN_TITLE,
        }
    }

    /// The command the item runs.
    pub fn action(self) -> Box<dyn Action> {
        match self {
            Self::CloseTab => Box::new(CloseTab),
            Self::CloseOthers => Box::new(CloseOtherTabs),
            Self::CloseToRight => Box::new(CloseTabsToRight),
            Self::CloseAll => Box::new(super::arrange::ClosePanelGroup),
            Self::MoveToNewWindow => Box::new(MoveTabToNewWindow),
            Self::Pin => Box::new(PinTab),
            Self::Unpin => Box::new(UnpinTab),
        }
    }
}

/// The tab menu for the tab at `ix` of a group whose tabs are pinned as
/// `pinned` says, left to right: every item in order, each with whether it's
/// disabled. Close Other Tabs and Close All Tabs are disabled with no other
/// unpinned tab in the group, and Close Tabs to the Right with no unpinned tab
/// right of this one; Pin Tab reads Unpin Tab on a pinned tab.
pub fn items(pinned: &[bool], ix: usize) -> Vec<(TabMenuItem, bool)> {
    let others_unpinned = pinned
        .iter()
        .enumerate()
        .any(|(other, &pinned)| other != ix && !pinned);
    let right_unpinned = pinned.iter().skip(ix + 1).any(|&pinned| !pinned);
    let pin = if pinned.get(ix).copied().unwrap_or(false) {
        TabMenuItem::Unpin
    } else {
        TabMenuItem::Pin
    };
    vec![
        (TabMenuItem::CloseTab, false),
        (TabMenuItem::CloseOthers, !others_unpinned),
        (TabMenuItem::CloseToRight, !right_unpinned),
        (TabMenuItem::CloseAll, !others_unpinned),
        (TabMenuItem::MoveToNewWindow, false),
        (pin, false),
    ]
}

const CLOSE_TAB_TITLE: &str = "Close Tab";
const CLOSE_OTHERS_TITLE: &str = "Close Other Tabs";
const CLOSE_RIGHT_TITLE: &str = "Close Tabs to the Right";
const MOVE_TITLE: &str = "Move to New Window";
const PIN_TITLE: &str = "Pin Tab";
const UNPIN_TITLE: &str = "Unpin Tab";

/// The tab commands, global like the other tab commands (`ui::panel::tabs`)
/// and acting on the focused tab; no default key but the menu's own. In the
/// View menu's panel layout group, beside Open in New Panel.
pub fn register_commands(registry: &mut CommandRegistry) {
    let layout = Some(MenuSlot::View(ViewGroup::PanelLayout));
    let mut register = |id, title, default_binding, action: Box<dyn Action>, menu| {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: None,
            action,
            menu,
        })
    };
    register("tab.close", CLOSE_TAB_TITLE, "", Box::new(CloseTab), None);
    register(
        "tab.close_others",
        CLOSE_OTHERS_TITLE,
        "",
        Box::new(CloseOtherTabs),
        None,
    );
    register(
        "tab.close_right",
        CLOSE_RIGHT_TITLE,
        "",
        Box::new(CloseTabsToRight),
        None,
    );
    register(
        "tab.move_to_new_window",
        MOVE_TITLE,
        "",
        Box::new(MoveTabToNewWindow),
        layout,
    );
    register("tab.pin", PIN_TITLE, "", Box::new(PinTab), None);
    register("tab.unpin", UNPIN_TITLE, "", Box::new(UnpinTab), None);
    // A dock chord, like the other `cmd-k` ones, so a text field keeps
    // `cmd-k` to itself.
    registry.register(Command {
        id: "tab.menu",
        title: "Open Tab Menu",
        default_binding: OPEN_TAB_MENU_KEY,
        context: Some(super::arrange::DOCK_COMMANDS_CONTEXT),
        action: Box::new(OpenTabMenu),
        menu: None,
    });
}

/// Where each panel's tab title was last drawn, by panel - what the keyboard's
/// Open Tab Menu anchors the menu to. Written by the title as it paints.
#[derive(Default)]
pub struct TabTitleBounds(HashMap<EntityId, Bounds<Pixels>>);

impl Global for TabTitleBounds {}

impl TabTitleBounds {
    /// Records that `panel`'s title was drawn at `bounds`.
    pub fn record(panel: EntityId, bounds: Bounds<Pixels>, cx: &mut App) {
        cx.default_global::<Self>().0.insert(panel, bounds);
    }

    /// Where `panel`'s title was last drawn, if it has been.
    pub fn of(panel: EntityId, cx: &App) -> Option<Bounds<Pixels>> {
        cx.try_global::<Self>()?.0.get(&panel).copied()
    }
}

#[cfg(test)]
mod tests;
