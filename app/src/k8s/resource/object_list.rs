//! The list panel every non-Pod kind opens (`standard-resource-panels` design D1):
//! one implementation over `Api<DynamicObject>`, with the kind's `ApiResource`
//! from discovery, so it covers built-in kinds and CRDs alike. This file declares
//! and re-exports; each submodule owns one concern.

// UNWIRED: `ui::nav` (`standard-resource-panels` 1.4) is the first to open the
// panel, and the window (1.5) the first to handle its open action; until then
// only tests reach these.
#[allow(dead_code)]
mod commands;
#[allow(dead_code)]
mod panel;
#[allow(dead_code)]
mod render;
mod row;
mod store;
#[allow(dead_code)]
mod table;
mod watch;

#[allow(unused_imports)]
pub use commands::{OpenListedObject, PANEL_KEY_CONTEXT, register_commands};
#[allow(unused_imports)]
pub use panel::ObjectListPanel;
pub use store::ObjectsTable;
pub use watch::watch_kind;
