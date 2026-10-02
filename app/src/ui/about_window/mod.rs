//! The About window - what it shows and how it opens
//! (`openspec/changes/about-window`).
//!
//! It is a window rather than a dialog: an About box that blocks the window
//! it opened over is not what any desktop platform does. A single instance -
//! choosing About again raises the one that is open.
//!
//! `build_info` answers which binary is running, `window` opens and owns the
//! window, `pane` draws it.

mod build_info;
mod pane;
mod window;

pub(crate) use build_info::{build_identifier, version};
pub(crate) use window::register_about_action;
