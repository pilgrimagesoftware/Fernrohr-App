//! A shell in a running container (`k9s-remaining-keybindings` section 3):
//! [`bridge`] pumps an exec's streams to and from the app, and [`panel`] is the
//! dock panel that shows its transcript and takes the user's input.
//!
//! One panel, one session: no terminal emulation and no PTY on this side. The
//! exec runs without a TTY, so the container's shell reads whole lines from
//! stdin; the panel sends a line on Enter and echoes it into the transcript.

mod bridge;
mod panel;
mod render;

pub use panel::{ExecPanel, ExecTarget, register_restore};

#[cfg(test)]
mod tests;
