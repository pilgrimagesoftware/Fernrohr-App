//! The events browser (`events-browser`): a live, filterable, searchable table
//! of a cluster context's retained events, opened from the Event kind and the
//! "Events" command.
//!
//! This module declares and re-exports. [`row`] turns one event into the row the
//! table shows, [`store`] keeps a context's rows current, and [`watch`] feeds it
//! from the shared typed Event watch (design D1). [`panel`] is the dock panel,
//! [`render`] draws it, [`table`] and [`columns`] are its table, and
//! [`restore`] saves and rebuilds it with the window's layout.

mod actions;
mod columns;
mod commands;
mod filters;
mod panel;
mod render;
pub(crate) mod restore;
mod row;
mod store;
mod table;
mod watch;

pub use commands::register_commands;
pub use panel::EventsPanel;
pub use restore::register_restore;
pub use row::EventRow;
pub use store::EventsTable;
pub use watch::watch_events;

#[cfg(test)]
mod tests;
