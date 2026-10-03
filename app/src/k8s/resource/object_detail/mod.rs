//! One object of any discovered kind in its own dock panel: its metadata, the
//! sections that matter for its kind, the events naming it, and its YAML.
//!
//! `resource-links` sections 5 and 6 (the `object-detail` capability). What a
//! reference leads to when its kind has no dedicated panel - which, today, is
//! every kind but Pod.
//!
//! [`commands`] owns the panel's actions, [`model`] the field model, [`fetch`]
//! the one-shot read, [`redact`] keeping Secret values out of everything the
//! panel stores, [`metadata`] the Overview section every kind gets,
//! [`sections`] the kind-specific ones, and [`panel`], [`render`] and
//! [`restore`] the dock panel itself.

mod commands;
mod fetch;
mod live;
mod metadata;
mod model;
mod panel;
mod redact;
mod render;
mod restore;
mod reveal;
mod sections;
#[cfg(test)]
mod tests;

pub use commands::register_commands;
pub use panel::ObjectDetailPanel;
pub use restore::{register_restore, target_from_state};
