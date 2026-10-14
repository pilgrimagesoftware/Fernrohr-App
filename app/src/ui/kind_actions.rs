//! The kind-specific actions' UI (`resource-specific-actions`): their commands
//! and the key contexts that gate them ([`commands`]), what a panel provides to
//! run them ([`KindHost`]), each action from prompt to result ([`flow`]), and
//! what only some need - the taint chooser ([`taints`]) and the token dialog
//! ([`token`]). The rules and cluster calls are `k8s::resource::kind_actions`'.

mod buttons;
pub(crate) mod commands;
mod edits;
mod flow;
mod host;
mod taints;
pub(crate) mod token;

pub(crate) use buttons::{MenuEntry, buttons, hints, menu_entries};
pub(crate) use commands::register_commands;
pub(crate) use host::{KindHost, KindTarget, add_key_contexts, on_kind_actions};
