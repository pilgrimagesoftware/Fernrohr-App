//! One object of any discovered kind in its own dock panel: its metadata, the
//! sections that matter for its kind, the events naming it, and its YAML.
//!
//! `resource-links` sections 5 and 6 (the `object-detail` capability). What a
//! reference leads to when its kind has no dedicated panel - which, today, is
//! every kind but Pod.
//!
//! [`commands`] owns the panel's actions, [`delete`] its Delete, [`logs`] its
//! pods' logs, [`model`] the field model, [`fetch`]
//! the one-shot read, [`redact`] keeping Secret values out of everything the
//! panel stores, [`metadata`] the Overview section every kind gets,
//! [`sections`] the kind-specific ones, [`node_pods`] the Node's live pods
//! region (#186), and [`panel`], [`render`] and [`restore`] the dock panel
//! itself.

mod commands;
mod delete;
mod edit;
mod edit_view;
mod fetch;
mod live;
mod logs;
mod metadata;
mod model;
mod node_pods;
mod panel;
mod redact;
mod render;
mod restore;
mod reveal;
mod sections;
#[cfg(test)]
mod tests;

pub use panel::ObjectDetailPanel;
pub use restore::{register_restore, target_from_state};

/// Every command this panel contributes: its own ([`commands`]) plus the
/// Node pods region's ([`node_pods`]) - one registry entry point, so a call
/// site can't wire one and forget the other.
pub fn register_commands(registry: &mut crate::command::CommandRegistry) {
    commands::register_commands(registry);
    node_pods::register_commands(registry);
}
