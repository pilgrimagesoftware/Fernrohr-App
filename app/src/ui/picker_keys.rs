//! The cluster picker's keyboard hint row: every picker action is reachable from the
//! keyboard as well as the mouse, and this row says how - the same visible-shortcut
//! convention the Pods panel uses. It owns only the hints, not the actions: Enter is
//! `Command`'s own confirm, and the tunnel and Tunnels-window shortcuts are the
//! app-wide `context.set_tunnel` and `tunnels.manage` commands.

use crate::ui::tunnels::TunnelsManage;
use crate::util::shell::SetContextTunnel;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;

/// The actions with no button of their own: "↑↓ Select  ⌘⇧B Tunnel". Connect and
/// Manage tunnels show their keys beside their buttons ([`with_key`]), so nothing is
/// listed twice.
pub fn key_hints(window: &mut Window, cx: &App) -> impl IntoElement {
    let arrows = div()
        .flex()
        .gap_0p5()
        .child(Kbd::new(Keystroke::parse("up").expect("valid keystroke")))
        .child(Kbd::new(Keystroke::parse("down").expect("valid keystroke")));
    div()
        .flex()
        .flex_wrap()
        .gap(crate::ui::space::spacing(cx).control_gap)
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(hint(arrows.into_any_element(), "Select"))
        .children(
            Kbd::binding_for_action(&SetContextTunnel, None, window)
                .map(|key| hint(key.into_any_element(), "Tunnel")),
        )
}

/// `button` with its shortcut shown beside it, when it has one.
pub fn with_key(button: impl IntoElement, key: Option<Kbd>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(button)
        .children(key)
}

/// Enter, the key that confirms the highlighted context.
pub fn enter_key() -> Kbd {
    Kbd::new(Keystroke::parse("enter").expect("valid keystroke"))
}

/// Whatever `tunnels.manage` is bound to now.
pub fn manage_tunnels_key(window: &mut Window) -> Option<Kbd> {
    Kbd::binding_for_action(&TunnelsManage, None, window)
}

fn hint(key: AnyElement, label: &'static str) -> impl IntoElement {
    div().flex().items_center().gap_1().child(key).child(label)
}
