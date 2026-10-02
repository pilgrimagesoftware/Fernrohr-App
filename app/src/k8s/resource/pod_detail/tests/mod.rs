//! The pod detail module's tests, split the same way the module is: the
//! projection, the event and managed-field formatting, the panel in a window,
//! and the fetch against a fixture API server.
//!
//! No file here uses `use super::*` from the production modules:
//! `gpui_kit::*` re-exports its own `test` macro, which would shadow
//! `core::prelude::v1::test` for the plain synchronous tests.

mod collapse;
mod config_fixture;
mod configuration;
mod container_detail;
mod container_view;
mod events;
mod fetch;
mod fixtures;
mod links;
mod live_events;
mod panel;
mod projection;
mod references;
