//! The object viewer's tests: the fetch against a fixture API server, the
//! Overview projection, the panel in a window (views, links, keys, the
//! not-found state), and dock restore.
//!
//! Explicit imports throughout rather than `use super::*`: `gpui_kit::*`
//! re-exports a `test` macro that would shadow the built-in one.

mod autoscaling;
mod cluster;
mod copy;
mod delete;
mod edit;
mod fetch;
mod fixtures;
mod header;
mod ingress_links;
mod live;
mod metadata;
mod network;
mod overview;
mod panel;
mod rbac;
mod secrets;
mod sections;
mod storage;
mod workloads;
mod yaml;
mod yaml_copy;
