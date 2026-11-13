//! Helm releases (`helm-releases`, #370): read from the Secrets Helm stores
//! each revision in. This module owns decoding a revision's payload
//! ([`decode`]), the table of a context's releases ([`table`]) and the watch
//! that fills it ([`watch`]), and the Helm Releases list panel ([`list`]); the
//! release detail panel and actions build on them.

pub(crate) mod actions;
mod decode;
pub(crate) mod detail;
pub(crate) mod list;
pub(crate) mod manifest;
mod table;
mod watch;

#[cfg(test)]
mod live_tests;

// UNWIRED(#370): the panels (helm-releases sections 3-4) are the first
// callers of the rest.
#[allow(unused_imports)]
pub(crate) use decode::{
    ChartMeta, DecodeError, HelmRelease, JsonErrorKind, ReleaseInfo, decode_release,
};
#[allow(unused_imports)]
pub(crate) use table::{Fetch, HelmReleasesTable, ReleaseKey, ReleaseRow, Revision, Summary};
pub(crate) use watch::{HelmScope, watch_helm_releases};
