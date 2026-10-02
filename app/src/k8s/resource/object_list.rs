//! The list panel every non-Pod kind opens (`standard-resource-panels` design D1):
//! one implementation over `Api<DynamicObject>`, with the kind's `ApiResource`
//! from discovery, so it covers built-in kinds and CRDs alike. This file declares
//! and re-exports; each submodule owns one concern.

// UNWIRED: the window (`standard-resource-panels` 1.5) is the first to handle
// the open action rows dispatch; until then only tests construct it.
#[allow(dead_code)]
mod commands;
mod panel;
mod render;
mod row;
mod store;
mod table;
mod watch;

#[allow(unused_imports)]
pub use commands::{OpenListedObject, PANEL_KEY_CONTEXT, register_commands};
pub use panel::ObjectListPanel;
pub use store::ObjectsTable;
pub use watch::watch_kind;
