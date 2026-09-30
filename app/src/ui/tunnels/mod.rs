//! Section 4 of the `tunnel-management-ui` change: the Tunnels window.
//!
//! Tunnels are app-global, not per-cluster, and must be reachable before any context
//! is connected, so this is a single-instance OS window rather than a cluster dock
//! panel (design.md decision 5) - reached from the picker's "Manage tunnels…" control
//! (`ui/picker.rs`), the app menu (`main.rs`), and the `tunnels.manage` command
//! (`list::register_commands`). It lists every tunnel with its usage count and running
//! state and lets the user create/edit/delete one; it never assigns a context to a
//! tunnel - that binding is set from the context side (`ui/picker_tunnel.rs`,
//! `util/shell.rs`'s `context.set_tunnel`), per proposal.md.

mod editor;
mod list;

pub use list::{TunnelsManage, open_or_focus, register_commands};
