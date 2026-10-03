//! One Pod's detail panel: a structured field list, or its raw YAML.
//!
//! A dock panel of its own rather than a block inside `PodsPanel`, so detail can
//! be viewed, moved, and closed like any other panel - and so two pods' details
//! can be open side by side. It reads the single `Pod` it was opened for with
//! `Api::get` rather than joining the all-namespaces watch: there is one object,
//! nothing to keep up to date beyond a re-fetch, and no reason to stream a whole
//! cluster's pods into a panel that shows one.
//!
//! The module is split by concern: [`commands`] owns the panel's actions and
//! keybindings, [`model`] the view model the projection produces, [`fields`]
//! and [`format`] the `Pod` -> field-list projection itself,
//! [`container_detail`] a container card's expanded fields, [`references`]
//! the objects that projection points at, [`fetch`] the
//! one-shot read, and [`panel`], [`field_view`], [`container_view`] and [`render`] the dock panel
//! that draws it.

mod commands;
mod configuration;
mod container_detail;
pub(super) mod container_view;
pub(super) mod events_tab;
mod fetch;
mod field_view;
mod fields;
mod format;
mod live;
pub(super) mod live_events;
pub(super) mod managed_fields_view;
mod model;
mod panel;
mod references;
mod render;
#[cfg(test)]
mod tests;
mod window_commands;
pub(crate) mod window_preference;

pub use commands::register_commands;
pub use model::DetailView;
pub use panel::{PodDetailPanel, register_restore};
