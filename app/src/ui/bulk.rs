//! The list panels' shared bulk-selection UI (`bulk-select-list-actions`):
//! which rows are checked and the checkbox that shows it ([`checked`]), the bar
//! offering what the checked rows can do ([`bar`]), the commands behind it
//! ([`commands`]), and each action's run from prompt to outcome ([`flow`],
//! [`prompt`], [`confirm`], [`outcome`]). A panel takes part by implementing
//! [`BulkHost`]. The cluster calls are `k8s::resource::bulk`'s.

pub(crate) mod bar;
mod checked;
pub(crate) mod commands;
pub(crate) mod confirm;
pub(crate) mod flow;
mod host;
mod key;
pub(crate) mod outcome;
pub(crate) mod prompt;

pub(crate) use checked::{CheckedRows, RowKey, with_checkbox};
pub(crate) use commands::register_commands;
pub(crate) use host::{BulkHost, BulkSelection, add_key_contexts, on_bulk_actions, render_bulk};
pub(crate) use outcome::BatchOutcome;
