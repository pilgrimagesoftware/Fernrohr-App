//! A shell in a running container (`embedded-exec-terminal`): a real terminal
//! over a kube exec with a TTY, so full-screen programs, Tab completion and
//! Ctrl-C work as they do in a local terminal.
//!
//! [`transport`] bridges the exec to `gpui-terminal`, which emulates, draws
//! and encodes input; [`theme`] gives the terminal the app's font and colours;
//! [`panel`] is the dock panel around it, and [`render`] what it draws.

mod panel;
mod render;
mod theme;
mod transport;

pub use panel::{ExecPanel, ExecTarget, register_restore};

#[cfg(test)]
mod tests;
