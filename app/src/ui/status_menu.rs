//! The menu bar status item's menu (quiet mode, #236): what it lists, as a
//! pure function of the kubeconfig's contexts ([`items`]), and what choosing
//! an entry does - dispatch the existing command it stands for ([`choose`]),
//! so the menu is one more route to them, not a parallel implementation.
//!
//! - **Contexts ▸** every kubeconfig context; one opens a main window
//!   connected to it.
//! - **Settings…**, **MCP…** (Settings at Agent Access), **Tunnel Manager…**
//! - a separator, then **Quit Fernrohr**.
//!
//! Drawing it is platform code: AppKit's `NSStatusItem` on macOS
//! ([`macos`]); nothing elsewhere yet (#336).

use crate::util::shell::OpenContextWindow;
use gpui_kit::{Action, App};

#[cfg(target_os = "macos")]
pub mod macos;

/// The Contexts submenu's title.
pub const CONTEXTS: &str = "Contexts";
/// The Contexts submenu's one, disabled item when the kubeconfig has none.
pub const NO_CONTEXTS: &str = "No contexts in your kubeconfig";

/// One entry of the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    /// The Contexts submenu: a [`Choice::Context`] per name, in kubeconfig
    /// order.
    Contexts(Vec<String>),
    /// An entry dispatching `choice`, titled `title`.
    Command {
        title: &'static str,
        choice: Choice,
    },
    Separator,
}

/// What an entry does when chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    /// Open a main window connected to this context.
    Context(String),
    Settings,
    /// Settings, at Agent Access: the MCP server's setting and agent setup.
    Mcp,
    TunnelManager,
    Quit,
}

impl Choice {
    /// The existing command's action this entry dispatches.
    pub fn action(&self) -> Box<dyn Action> {
        match self {
            Choice::Context(context) => Box::new(OpenContextWindow {
                context: context.clone(),
            }),
            Choice::Settings => Box::new(crate::ui::settings::OpenSettings),
            Choice::Mcp => Box::new(crate::ui::settings::OpenAgentAccess),
            Choice::TunnelManager => Box::new(crate::ui::tunnels::TunnelsManage),
            Choice::Quit => Box::new(crate::ui::menu::Quit),
        }
    }
}

/// The menu for `contexts`, top to bottom.
pub fn items(contexts: &[String]) -> Vec<Item> {
    vec![
        Item::Contexts(contexts.to_vec()),
        Item::Command {
            title: "Settings\u{2026}",
            choice: Choice::Settings,
        },
        Item::Command {
            title: "MCP\u{2026}",
            choice: Choice::Mcp,
        },
        Item::Command {
            title: "Tunnel Manager\u{2026}",
            choice: Choice::TunnelManager,
        },
        Item::Separator,
        Item::Command {
            title: "Quit Fernrohr",
            choice: Choice::Quit,
        },
    ]
}

/// Does what `choice` stands for, through its command's app-wide handler -
/// there may be no window to dispatch from.
pub fn choose(choice: &Choice, cx: &mut App) {
    cx.dispatch_action(choice.action().as_ref());
}

#[cfg(test)]
mod tests;
