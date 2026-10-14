//! The window's side of a panel tab's context menu (`panel-tab-menu`): the
//! tab commands, run against the focused tab - Close Tab, Close Other Tabs,
//! Close Tabs to the Right, Move to New Window, Pin and Unpin (Close All Tabs
//! is the dock's Close Group, `arrange`) - and the menu itself, opened by a
//! right-click on a tab or by Open Tab Menu for the focused one, which the
//! window owns and draws so the mouse and the keyboard open the same menu.
//!
//! An item selects the tab its menu was opened for and dispatches its
//! command; the command takes that tab as its target and finds its group
//! afresh (design.md's target resolution) - so a tab moved or closed since the
//! menu opened can't make it act on the wrong one.

use super::*;
use crate::config::saved_layouts::SavedLayout;
use crate::ui::panel::tab_menu::{
    CloseOtherTabs, CloseTab, CloseTabsToRight, MoveTabToNewWindow, OpenTabMenu, OpenTabMenuAt,
    PinTab, TabMenuItem, TabTitleBounds, UnpinTab, items,
};
use gpui_kit::component::dock::{DockAreaState, InsertTarget, NodeId, PanelInfo, PanelState};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};

/// The tab menu while it's open: the menu, where it opened, and the
/// subscription that forgets it once dismissed.
pub(super) struct TabMenu {
    menu: Entity<PopupMenu>,
    position: Point<Pixels>,
    _dismissed: Subscription,
}

/// A window's tab menu: the open menu, if any, and the tab a chosen item
/// targets - which the item's command takes in place of the focused tab, once.
/// Focusing the target tab instead isn't enough: a tab that wasn't on screen
/// isn't on the focus path until it's drawn, after the command has run.
#[derive(Default)]
pub(super) struct TabMenuState {
    open: Option<TabMenu>,
    target: std::cell::Cell<Option<PanelId>>,
}

impl TabMenuState {
    /// Whether the menu is open.
    #[cfg(test)]
    pub(super) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// The tab a menu item just chose, for its command to act on - once.
    pub(super) fn take_target(&self) -> Option<PanelId> {
        self.target.take()
    }
}

/// Whether the dock panel `panel` is pinned.
pub(super) fn is_pinned(dock_area: &Entity<DockArea>, panel: PanelId, cx: &App) -> bool {
    nav::opened_panel_for(dock_area.read(cx), panel, cx).is_some_and(|opened| opened.pinned(cx))
}

/// `panel`'s tab group and its tabs, left to right.
fn group_of(
    dock_area: &Entity<DockArea>,
    panel: PanelId,
    cx: &App,
) -> Option<(NodeId, Vec<PanelId>)> {
    let area = dock_area.read(cx);
    let node = crate::ui::panel::tabs::group_of(area, panel)?;
    Some((node, crate::ui::panel::tabs::tabs_of(area, node)?.panels))
}

impl MainWindow {
    /// The tab commands' and the tab menu's handlers, on the window's root.
    pub(super) fn with_tab_menu_actions(element: Div, cx: &mut Context<Self>) -> Div {
        element
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| this.close_tab(window, cx)))
            .on_action(cx.listener(|this, _: &CloseOtherTabs, window, cx| {
                this.close_around(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &CloseTabsToRight, window, cx| {
                this.close_around(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &PinTab, window, cx| this.pin(true, window, cx)))
            .on_action(cx.listener(|this, _: &UnpinTab, window, cx| this.pin(false, window, cx)))
            .on_action(cx.listener(|this, _: &MoveTabToNewWindow, window, cx| {
                this.move_to_new_window(window, cx)
            }))
            .on_action(cx.listener(|this, _: &OpenTabMenu, window, cx| {
                this.open_focused_tab_menu(window, cx)
            }))
            .on_action(cx.listener(|this, action: &OpenTabMenuAt, window, cx| {
                this.open_tab_menu(PanelId::from(action.panel), action.position, window, cx)
            }))
    }

    /// The open tab menu, drawn over the window at the point it opened.
    pub(super) fn tab_menu_layer(&self) -> Option<AnyElement> {
        let open = self.tab_menu.open.as_ref()?;
        Some(
            deferred(
                anchored()
                    .position(open.position)
                    .snap_to_window_with_margin(px(8.))
                    .child(open.menu.clone()),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }

    /// The dock and the tab the tab commands act on: the one a menu item just
    /// chose, else the focused tab.
    fn focused_tab(&self, window: &Window, cx: &App) -> Option<(Entity<DockArea>, PanelId)> {
        let WindowMode::Workspace { dock_area, .. } = &self.mode else {
            return None;
        };
        if let Some(target) = self.tab_menu.take_target()
            && dock_area.read(cx).panel(target).is_some()
        {
            return Some((dock_area.clone(), target));
        }
        let group = crate::ui::panel::tabs::focused_group(dock_area.read(cx), window, cx)?;
        Some((dock_area.clone(), *group.panels.get(group.active_ix)?))
    }

    /// Close Tab: the focused tab, asking first if it would lose something.
    fn close_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((dock_area, panel)) = self.focused_tab(window, cx) else {
            return;
        };
        if super::arrange::ask_before_closing(&dock_area, panel, window, cx) {
            return;
        }
        super::arrange::close_panels(&dock_area, &[panel], window, cx);
    }

    /// Close Other Tabs, or - `to_right` - Close Tabs to the Right: the focused
    /// tab's unpinned siblings, or those right of it, asking once first if any
    /// would lose something.
    fn close_around(&mut self, to_right: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((dock_area, panel)) = self.focused_tab(window, cx) else {
            return;
        };
        let Some((_, tabs)) = group_of(&dock_area, panel, cx) else {
            return;
        };
        let Some(ix) = tabs.iter().position(|&tab| tab == panel) else {
            return;
        };
        let closing: Vec<PanelId> = tabs
            .iter()
            .enumerate()
            .filter(|&(other, _)| if to_right { other > ix } else { other != ix })
            .map(|(_, &tab)| tab)
            .filter(|&tab| !is_pinned(&dock_area, tab, cx))
            .collect();
        if closing.is_empty() {
            return;
        }
        let lead = format!("This closes {} tabs.", closing.len());
        let names = if to_right {
            ("Close Tabs to the Right?", "Close Tabs", "close-right")
        } else {
            ("Close Other Tabs?", "Close Tabs", "close-others")
        };
        super::arrange::close_confirming(dock_area, closing, names, lead, window, cx);
    }

    /// Pin Tab, or - `pinned` false - Unpin Tab, on the focused tab: flips its
    /// state and moves it to the end of its group's pinned tabs, so pinned tabs
    /// stay first.
    fn pin(&mut self, pinned: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((dock_area, panel)) = self.focused_tab(window, cx) else {
            return;
        };
        let Some(opened) = nav::opened_panel_for(dock_area.read(cx), panel, cx) else {
            return;
        };
        if opened.pinned(cx) == pinned {
            return;
        }
        opened.set_pinned(pinned, cx);
        let Some((node, tabs)) = group_of(&dock_area, panel, cx) else {
            return;
        };
        let pinned_others = tabs
            .iter()
            .filter(|&&tab| tab != panel && is_pinned(&dock_area, tab, cx))
            .count();
        let target = InsertTarget::Tabs {
            node,
            ix: Some(pinned_others),
            activate: true,
        };
        dock_area.update(cx, |area, cx| area.move_panel(panel, target, window, cx));
        Self::focus_panel_now(&dock_area, panel, window, cx);
        cx.notify();
    }

    fn focus_panel_now(
        dock_area: &Entity<DockArea>,
        panel: PanelId,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(view) = dock_area.read(cx).panel(panel) {
            window.focus(&view.focus_handle(cx), cx);
        }
    }

    /// Move to New Window: the focused tab, as the only panel of a new window
    /// on its context, restored from its own saved state - its scope, its
    /// list's filter and sort, its pinned state - then closed here. As the
    /// new window's only panel it's that window's first instance, so its
    /// instance number is dropped (`open-panel-new-instance`). Asks first, as
    /// closing would, if it would lose something here: a running shell can't
    /// move, so it ends.
    fn move_to_new_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((dock_area, panel)) = self.focused_tab(window, cx) else {
            return;
        };
        let Some(view) = dock_area.read(cx).panel(panel) else {
            return;
        };
        let mut state = view.dump(cx);
        let PanelInfo::Panel(info) = &mut state.info else {
            return;
        };
        if let Some(info) = info.as_object_mut() {
            info.remove("instance");
        }
        let Some(context_name) = info["context_name"].as_str().map(str::to_string) else {
            return;
        };
        let version = dock_area.read(cx).dump(cx).version;
        let layout = SavedLayout {
            version: crate::consts::SAVED_LAYOUT_SCHEMA_VERSION,
            name: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            contexts: vec![context_name.clone()],
            dock: DockAreaState {
                version,
                center: PanelState {
                    panel_name: "TabPanel".into(),
                    children: vec![state],
                    info: PanelInfo::Tabs { active_index: 0 },
                },
                left_dock: None,
                right_dock: None,
                bottom_dock: None,
            },
            resource_panel_width: self.resource_width().map(f32::from),
            window_width: f32::from(window.bounds().size.width),
            window_height: f32::from(window.bounds().size.height),
        };
        let lead = "This moves the tab to a new window.".to_string();
        let names = ("Move to New Window?", "Move", "move-tab");
        let move_it = {
            let dock_area = dock_area.clone();
            move |window: &mut Window, cx: &mut App| {
                super::arrange::close_panels(&dock_area, &[panel], window, cx);
                open_window_with(context_name.clone(), layout.clone(), window, cx);
            }
        };
        let warned = super::arrange::close_warning(dock_area.read(cx), panel, cx).is_some();
        if warned {
            super::arrange::confirm_then(&dock_area, &[panel], names, lead, move_it, window, cx);
        } else {
            move_it(window, cx);
        }
    }

    /// Open Tab Menu: the focused tab's menu, under its tab - or, before its
    /// title has been drawn, at the window's top left.
    fn open_focused_tab_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((dock_area, panel)) = self.focused_tab(window, cx) else {
            return;
        };
        let entity = dock_area
            .read(cx)
            .panel(panel)
            .map(|view| view.view().entity_id());
        let position = entity
            .and_then(|entity| TabTitleBounds::of(entity, cx))
            .map(|bounds| bounds.bottom_left())
            .unwrap_or_else(|| point(px(16.), px(48.)));
        self.open_tab_menu(panel, position, window, cx);
    }

    /// Opens the tab menu for `panel` at `position`, focused, so the arrows
    /// move through it and Enter runs the highlighted item.
    pub(super) fn open_tab_menu(
        &mut self,
        panel: PanelId,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace { dock_area, .. } = &self.mode else {
            return;
        };
        let dock_area = dock_area.clone();
        let Some((_, tabs)) = group_of(&dock_area, panel, cx) else {
            return;
        };
        let Some(ix) = tabs.iter().position(|&tab| tab == panel) else {
            return;
        };
        let pinned: Vec<bool> = tabs
            .iter()
            .map(|&tab| is_pinned(&dock_area, tab, cx))
            .collect();
        let this = cx.weak_entity();
        let menu = PopupMenu::build(window, cx, move |menu, _window, _cx| {
            items(&pinned, ix)
                .into_iter()
                .fold(menu, |menu, (item, disabled)| {
                    let this = this.clone();
                    menu.item(
                        PopupMenuItem::new(item.label())
                            .disabled(disabled)
                            .on_click(move |_event, window, cx| {
                                let _ = this.update(cx, |this, cx| {
                                    this.run_tab_item(panel, item, window, cx)
                                });
                            }),
                    )
                })
        });
        let dismissed = cx.subscribe_in(&menu, window, |this, _menu, _: &DismissEvent, _, cx| {
            this.tab_menu.open = None;
            cx.notify();
        });
        // Focused once the event that opened it is done with: a right-click's
        // own press would otherwise hand focus back to the tab it was on.
        let focus = menu.read(cx).focus_handle(cx);
        focus.focus(window, cx);
        window.defer(cx, move |window, cx| focus.focus(window, cx));
        self.tab_menu.open = Some(TabMenu {
            menu,
            position,
            _dismissed: dismissed,
        });
        cx.notify();
    }

    /// Runs `item` for `panel`: closes the menu, focuses `panel`'s tab - if it's
    /// still in the dock - and dispatches the item's command from there.
    pub(super) fn run_tab_item(
        &mut self,
        panel: PanelId,
        item: TabMenuItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tab_menu.open = None;
        cx.notify();
        let WindowMode::Workspace { dock_area, .. } = &self.mode else {
            return;
        };
        let dock_area = dock_area.clone();
        if dock_area.read(cx).panel(panel).is_none() {
            return;
        }
        dock_area.update(cx, |area, cx| area.select_panel(panel, window, cx));
        // Dispatched from the window's own root, which is always on the focus
        // path - a tab that wasn't on screen isn't on it until it's drawn. The
        // command takes `panel` as its target, not the focus, as its first
        // step; the tab takes focus once it's drawn.
        self.tab_menu.target.set(Some(panel));
        self.focus_handle.focus(window, cx);
        window.dispatch_action(item.action(), cx);
        window
            .on_next_frame(move |window, cx| Self::focus_panel_now(&dock_area, panel, window, cx));
        window.refresh();
    }
}

/// Opens a window on `context_name` and loads `layout` into it - Move to New
/// Window's destination. Deferred: opening a window from inside another's
/// update.
fn open_window_with(context_name: String, layout: SavedLayout, window: &mut Window, cx: &mut App) {
    window.defer(cx, move |_window, cx| {
        let opened = super::open_window(
            cx,
            WindowLayout {
                contexts: vec![context_name],
                width: layout.window_width,
                height: layout.window_height,
                ..Default::default()
            },
        );
        let _ = opened.update(cx, |view, window, cx| {
            let Ok(root) = view.downcast::<gpui_kit::component::Root>() else {
                return;
            };
            let Ok(main) = root.read(cx).view().clone().downcast::<MainWindow>() else {
                return;
            };
            main.update(cx, |main, cx| main.load_replace(layout, window, cx));
        });
    });
}

#[cfg(test)]
#[path = "tab_menu_tests.rs"]
mod tests;
