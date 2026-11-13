//! The release detail panel (helm-releases section 4): one release's Overview,
//! History and Notes, live from the release watch over its namespace.
//!
//! [`panel`] holds its state and data flow, [`fetch`] decodes one revision off
//! the UI thread, [`render`] draws it, [`commands`] are its tab keys, and
//! [`restore`] saves and rebuilds it with the window's layout.

mod commands;
mod fetch;
mod panel;
mod render;
pub(crate) mod restore;

pub(crate) use commands::register_commands;
pub use panel::HelmReleasePanel;
#[cfg(test)]
pub(crate) use panel::ReleaseTab;
pub use restore::register_restore;

#[cfg(test)]
mod tests;
