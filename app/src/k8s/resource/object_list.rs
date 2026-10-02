//! The list panel every non-Pod kind opens (`standard-resource-panels` design D1):
//! one implementation over `Api<DynamicObject>`, with the kind's `ApiResource`
//! from discovery, so it covers built-in kinds and CRDs alike. This file declares
//! and re-exports; each submodule owns one concern.

mod columns;
mod commands;
mod panel;
mod render;
pub(crate) mod restore;
mod row;
mod store;
mod table;
mod watch;

pub(crate) use commands::LIST_KEY_CONTEXT;
pub use commands::{OpenListedObject, register_commands};
pub use panel::ObjectListPanel;
pub use restore::register_restore;
pub use store::ObjectsTable;
pub use watch::watch_kind;
