//! Proceed and Cancel for a manual tunnel awaiting confirmation
//! (`manual-confirmation-tunnels` D7): two registered commands, so each is in the
//! palette, has a key `keymap.toml` can rebind, and is reachable without the mouse.
//!
//! With one tunnel waiting, a command answers it at once. With several, it opens
//! [`picker`] to ask which. With none it does nothing. The capsule menu and the
//! cluster picker's status line offer the same two answers for their own tunnel;
//! every route ends in `ManualConfirmations::resolve`.

use crate::command::{Command, CommandRegistry, ContextGroup, MenuSlot};
use crate::tunnel::manual::{Decision, ManualConfirmations, Prompted};
use gpui_kit::*;

pub(crate) mod picker;

actions!(manual_tunnel, [ProceedManualTunnel, CancelManualTunnel]);

/// The registered commands' ids and default keys.
pub(crate) const PROCEED_COMMAND_ID: &str = "tunnel.manual.proceed";
pub(crate) const CANCEL_COMMAND_ID: &str = "tunnel.manual.cancel";
const PROCEED_KEY: &str = "secondary-alt-p";
const CANCEL_KEY: &str = "secondary-alt-c";

/// Registers Proceed with Manual Tunnel and Cancel Manual Tunnel, available
/// everywhere and listed with the tunnel commands in the menu.
pub fn register_commands(registry: &mut CommandRegistry) {
    for (id, title, default_binding, action) in [
        (
            PROCEED_COMMAND_ID,
            "Proceed with Manual Tunnel",
            PROCEED_KEY,
            Box::new(ProceedManualTunnel) as Box<dyn Action>,
        ),
        (
            CANCEL_COMMAND_ID,
            "Cancel Manual Tunnel",
            CANCEL_KEY,
            Box::new(CancelManualTunnel),
        ),
    ] {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: None,
            action,
            menu: Some(MenuSlot::Context(ContextGroup::Tunnels)),
        });
    }
}

/// The app-wide handlers, and the desktop notification each new prompt posts.
/// Called once at startup, after `ManualConfirmations::init`.
pub fn register_handler(cx: &mut App) {
    if let Some(confirmations) = ManualConfirmations::entity(cx) {
        cx.subscribe(&confirmations, |_, prompted: &Prompted, cx| {
            announce(prompted, cx)
        })
        .detach();
    }
    cx.on_action(|_: &ProceedManualTunnel, cx: &mut App| answer(Decision::Proceed, cx));
    cx.on_action(|_: &CancelManualTunnel, cx: &mut App| answer(Decision::Cancel, cx));
}

/// Answers the one waiting tunnel with `decision`, or asks which when several wait.
fn answer(decision: Decision, cx: &mut App) {
    // Deferred: a command runs while its window is mid-update.
    cx.defer(move |cx| {
        let waiting: Vec<String> = ManualConfirmations::entity(cx)
            .map(|entity| {
                entity
                    .read(cx)
                    .pending()
                    .iter()
                    .map(|entry| entry.tunnel_id.clone())
                    .collect()
            })
            .unwrap_or_default();
        match waiting.as_slice() {
            [] => {}
            [tunnel_id] => {
                ManualConfirmations::resolve(cx, tunnel_id, decision);
            }
            _ => {
                let Some(window) = asking_window(cx) else {
                    return;
                };
                let _ = window.update(cx, |_, window, cx| picker::open(decision, window, cx));
            }
        }
    });
}

/// Posts the one desktop notification a new prompt gets (design.md D6): the tunnel,
/// its instruction, and who is waiting. Activating it brings Fernrohr forward with
/// the window to answer in. The status bar and the picker carry the same prompt, so
/// a notification the platform refuses loses nothing.
fn announce(prompted: &Prompted, cx: &mut App) {
    let (title, body) = notification_text(prompted);
    let window = asking_window(cx);
    crate::notify::post(
        title,
        body,
        move |cx| match window {
            Some(window) => crate::notify::focus(window)(cx),
            None => cx.activate(true),
        },
        cx,
    );
}

/// A prompt's notification title and body.
pub(crate) fn notification_text(prompted: &Prompted) -> (String, String) {
    let title = format!("{} is waiting for you", prompted.name);
    let instruction = prompted
        .message
        .clone()
        .unwrap_or_else(|| "Bring its network path up, then choose Proceed in Fernrohr.".into());
    let body = match prompted.contexts.as_slice() {
        [] => instruction,
        contexts => format!("{instruction}\nWaiting: {}", contexts.join(", ")),
    };
    (title, body)
}

/// The window to ask in: the active one if it is a main window, else the first.
fn asking_window(cx: &App) -> Option<AnyWindowHandle> {
    let is_main = |handle: &AnyWindowHandle| crate::util::shell::is_main_window(*handle, cx);
    cx.active_window()
        .filter(is_main)
        .or_else(|| cx.windows().into_iter().find(is_main))
}

#[cfg(test)]
mod tests;
