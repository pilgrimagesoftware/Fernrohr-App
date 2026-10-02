//! Where the Resource panel sits and whether it shows: the window edge it's on
//! (`cluster-picker-and-navigation` 11.2) and its collapsed state (11.3), both per
//! window and not saved - the side a new window starts on is
//! [`super::side_preference`]'s. This module owns the two commands and the header
//! buttons that dispatch them; `MainWindow` owns the state and acts on them, since
//! it lays out the panel beside the dock.

use crate::command::{Command, CommandRegistry, MenuSlot};
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

actions!(resource_panel, [ToggleResourcePanel, MoveResourcePanel]);

pub(crate) const TOGGLE_COMMAND_ID: &str = "resource.toggle_panel";
pub(crate) const TOGGLE_DEFAULT_BINDING: &str = "cmd-b";
pub(crate) const MOVE_COMMAND_ID: &str = "resource.move_panel";
pub(crate) const MOVE_DEFAULT_BINDING: &str = "cmd-alt-b";

pub use crate::config::ui::ResourceSide;

impl ResourceSide {
    /// The other edge.
    pub fn other(self) -> Self {
        match self {
            ResourceSide::Left => ResourceSide::Right,
            ResourceSide::Right => ResourceSide::Left,
        }
    }

    /// The icon for collapsing a panel on this edge.
    fn collapse_icon(self) -> IconName {
        match self {
            ResourceSide::Left => IconName::PanelLeftClose,
            ResourceSide::Right => IconName::PanelRightClose,
        }
    }

    /// The icon for bringing back a panel collapsed on this edge.
    fn expand_icon(self) -> IconName {
        match self {
            ResourceSide::Left => IconName::PanelLeftOpen,
            ResourceSide::Right => IconName::PanelRightOpen,
        }
    }
}

/// Registers both as global commands in the View menu, so the palette and the menu
/// reach them from anywhere in the window - including while the panel is collapsed,
/// when it has no buttons to click.
pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: TOGGLE_COMMAND_ID,
        title: "Collapse/Expand Resource Panel",
        default_binding: TOGGLE_DEFAULT_BINDING,
        context: None,
        action: Box::new(ToggleResourcePanel),
        menu: Some(MenuSlot::View),
    });
    registry.register(Command {
        id: MOVE_COMMAND_ID,
        title: "Move Resource Panel to Other Side",
        default_binding: MOVE_DEFAULT_BINDING,
        context: None,
        action: Box::new(MoveResourcePanel),
        menu: Some(MenuSlot::View),
    });
}

/// The header's two buttons - move to the other side, collapse - each the mouse
/// route to its command. Tab stops, so Tab and Space reach them too.
pub(super) fn header_buttons(side: ResourceSide) -> impl IntoElement {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .child(
            Button::new("resource-panel-move")
                .icon(IconName::ArrowLeftRight)
                .xsmall()
                .ghost()
                .tooltip("Move Resource Panel to Other Side")
                .on_click(|_, window, cx| window.dispatch_action(Box::new(MoveResourcePanel), cx)),
        )
        .child(
            Button::new("resource-panel-collapse")
                .icon(side.collapse_icon())
                .xsmall()
                .ghost()
                .tooltip("Collapse Resource Panel")
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(ToggleResourcePanel), cx)
                }),
        )
}

/// What a collapsed panel leaves behind on its edge: a slim strip with the one
/// button that brings it back.
pub fn collapsed_strip(side: ResourceSide, cx: &App) -> impl IntoElement {
    use gpui_kit::component::ActiveTheme as _;
    let theme = cx.theme();
    div()
        .h_full()
        .flex_none()
        .flex()
        .flex_col()
        .items_center()
        .py(crate::ui::space::spacing(cx).control_gap)
        .bg(theme.tokens.sidebar)
        .map(|strip| match side {
            ResourceSide::Left => strip.border_r_1(),
            ResourceSide::Right => strip.border_l_1(),
        })
        .border_color(theme.sidebar_border)
        .child(
            Button::new("resource-panel-expand")
                .icon(side.expand_icon())
                .xsmall()
                .ghost()
                .tooltip("Expand Resource Panel")
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(ToggleResourcePanel), cx)
                }),
        )
}
