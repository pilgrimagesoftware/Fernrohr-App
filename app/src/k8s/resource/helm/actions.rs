//! Helm release actions (helm-releases section 8): Rollback and Uninstall,
//! run through the user's own `helm` (`helm_cli`, ADR-0002) behind
//! confirmations, from the release list and the release detail panel.
//!
//! This module owns what both panels share: the commands ([`commands`]), the
//! confirm-then-run flow and its progress and failure state ([`flow`]),
//! counting a manifest's objects for Uninstall's confirmation ([`manifest`]),
//! and the banner and buttons that show them ([`view`]). Where each action is
//! offered, and which release it targets, stays with the panels.

mod commands;
mod flow;
mod manifest;
mod view;

pub(crate) use commands::{
    ROLLBACK_KEY, RollbackRelease, UNINSTALL_KEY, UninstallRelease, register_commands,
};
pub(crate) use flow::{
    ActionHost, ActionState, RevisionLine, Rollback, confirm_rollback, confirm_uninstall, refuse,
};
pub(crate) use view::{BannerIds, action_button, status_banner};

#[cfg(test)]
mod tests;
