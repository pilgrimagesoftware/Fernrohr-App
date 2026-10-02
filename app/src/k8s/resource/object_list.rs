//! The list panel every non-Pod kind opens (`standard-resource-panels` design D1):
//! one implementation over `Api<DynamicObject>`, with the kind's `ApiResource`
//! from discovery, so it covers built-in kinds and CRDs alike. This file declares
//! and re-exports; each submodule owns one concern.

mod row;
mod store;
mod watch;

// UNWIRED: `ObjectListPanel` (`standard-resource-panels` 1.3) is the first
// caller; until then only tests use it.
#[allow(unused_imports)]
pub use row::ObjectRow;
pub use store::ObjectsTable;
pub use watch::watch_kind;
