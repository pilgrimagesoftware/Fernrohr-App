//! Agent setup from the keyboard (`agent-mcp`: Copying agent setup commands):
//! Copy MCP Setup Command, a palette command, opens [`picker`] - the four
//! harnesses - and copies the chosen one's registration command. With no
//! command to offer (an unstable executable path, or no endpoint on this
//! platform) it opens Settings at Agent access instead, which says why.
//!
//! [`setup`] is what both this and the Settings section show:
//! `mcp::setup::AgentSetup::current`, unless a test set its own.

use crate::command::{Command, CommandRegistry};
use crate::mcp::setup::AgentSetup;
use gpui_kit::*;

pub(crate) mod picker;

actions!(agent_setup, [CopyMcpSetupCommand]);

/// The registered command's id.
pub(crate) const COPY_COMMAND_ID: &str = "mcp.copy_setup_command";

/// What agent setup offers in this app: this process's own, unless a test
/// set one with [`set_setup`].
pub(crate) fn setup(cx: &App) -> AgentSetup {
    cx.try_global::<SetupOverride>()
        .map(|setup| setup.0.clone())
        .unwrap_or_else(|| AgentSetup::current().clone())
}

/// Shows `setup` instead of this process's own - a test binary runs from
/// `target/debug`, which reads as unstable.
#[cfg(test)]
pub(crate) fn set_setup(setup: AgentSetup, cx: &mut App) {
    cx.set_global(SetupOverride(setup));
}

struct SetupOverride(AgentSetup);

impl Global for SetupOverride {}

/// Registers Copy MCP Setup Command: palette-only, with no default key.
pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: COPY_COMMAND_ID,
        title: "Copy MCP Setup Command",
        default_binding: "",
        context: None,
        action: Box::new(CopyMcpSetupCommand),
        menu: None,
    });
}

/// The app-wide handler. Called once at startup.
pub fn register_handler(cx: &mut App) {
    cx.on_action(|_: &CopyMcpSetupCommand, cx: &mut App| {
        // Deferred: the palette runs it while its window is mid-update.
        cx.defer(|cx| {
            if !matches!(setup(cx), AgentSetup::Ready { .. }) {
                crate::ui::settings::open_at(crate::ui::settings::Section::AgentAccess, cx);
                return;
            }
            let is_main =
                |handle: &AnyWindowHandle| crate::util::shell::is_main_window(*handle, cx);
            let Some(window) = cx
                .active_window()
                .filter(is_main)
                .or_else(|| cx.windows().into_iter().find(is_main))
            else {
                return;
            };
            let _ = window.update(cx, |_, window, cx| picker::open(window, cx));
        });
    });
}

#[cfg(test)]
mod tests;
