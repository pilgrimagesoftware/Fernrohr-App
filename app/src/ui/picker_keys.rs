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

/// "↵ Connect  ⌘⇧B Tunnel  ⌘⇧T Manage tunnels", using whatever keys are bound now.
pub fn key_hints(window: &mut Window, cx: &App) -> impl IntoElement {
    let hint = |key: Option<Kbd>, label: &'static str| {
        div()
            .flex()
            .items_center()
            .gap_1()
            .children(key)
            .child(label)
    };
    let enter = Kbd::new(Keystroke::parse("enter").expect("valid keystroke"));
    div()
        .flex()
        .flex_wrap()
        .gap_3()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(hint(Some(enter), "Connect"))
        .child(hint(
            Kbd::binding_for_action(&SetContextTunnel, None, window),
            "Tunnel",
        ))
        .child(hint(
            Kbd::binding_for_action(&TunnelsManage, None, window),
            "Manage tunnels",
        ))
}
