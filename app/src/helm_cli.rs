//! Driving the user's `helm` 3 CLI for Helm release actions (`helm-releases`
//! D8, ADR-0002): rollback and uninstall run `helm` itself rather than a
//! reimplementation of its release engine.
//!
//! This module owns finding the binary ([`locate`]), pointing it at a
//! connection's effective endpoint through a per-run kubeconfig
//! ([`kubeconfig`]), running it off the UI thread in its own process group
//! ([`run`]), and the Settings preference naming its path ([`preference`]).
//! [`start`] ties them together for one run. Which actions exist and how they
//! are confirmed is the release panels' business, not this module's.

mod kubeconfig;
mod locate;
mod preference;
mod run;
mod start;
#[cfg(test)]
mod test_helm;

pub(crate) use kubeconfig::sweep_stale as sweep_stale_kubeconfigs;
#[cfg(test)]
pub(crate) use locate::{HelmBinary, requested, set_status};
pub(crate) use locate::{HelmStatus, status};
pub(crate) use preference::{current as current_path, init, set_path};
// UNWIRED(#370): section 8's actions use these; nothing outside tests does yet.
#[allow(unused_imports)]
pub(crate) use run::{HelmFailure, HelmOutput, ReleaseKey};
#[allow(unused_imports)]
pub(crate) use start::{HelmUnavailable, availability, start};
