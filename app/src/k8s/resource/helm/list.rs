//! The Helm Releases list panel (helm-releases section 3): a live table of a
//! context's releases in the panel's namespace scope, opened from the
//! Resource panel's Helm Releases row and the "Helm Releases" command.
//!
//! This module declares and re-exports. [`row`] turns a release into the row
//! the table shows, [`columns`] and [`table`] are its table, [`panel`] is the
//! dock panel and [`render`] draws it, [`commands`] are its shortcuts, and
//! [`restore`] saves and rebuilds it with the window's layout.

mod actions;
mod columns;
mod commands;
mod panel;
mod render;
pub(crate) mod restore;
mod row;
mod table;

pub(crate) use columns::status_tone;
pub(crate) use commands::{KEY_CONTEXT as LIST_KEY_CONTEXT, OpenHelmRelease, register_commands};
pub use panel::HelmReleasesPanel;
pub(crate) use render::KEY_CONTEXT_GROUP;
pub use restore::register_restore;

#[cfg(test)]
mod tests;
