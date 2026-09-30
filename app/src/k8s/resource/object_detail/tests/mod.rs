//! The object viewer's tests: the fetch against a fixture API server, the
//! Overview projection, the panel in a window (views, links, keys, the
//! not-found state), and dock restore.
//!
//! Explicit imports throughout rather than `use super::*`: `gpui_kit::*`
//! re-exports a `test` macro that would shadow the built-in one.

mod fetch;
mod fixtures;
mod overview;
mod panel;
mod secrets;
mod sections;
mod workloads;
