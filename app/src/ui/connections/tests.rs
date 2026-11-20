//! The Connections window against real windows and the real keymap: what it
//! lists (`list`), the selection and the three actions by real keystrokes
//! (`keys`), the icon buttons by clicks and hovers (`buttons`), and the
//! commands' registration (`commands`).
//!
//! Named imports, not `use super::*`: see `util/shell.rs` on the macro-expansion
//! budget.

mod agents;
mod buttons;
mod commands;
mod keys;
mod list;
mod support;
