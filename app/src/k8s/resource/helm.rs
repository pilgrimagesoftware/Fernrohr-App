//! Helm releases (`helm-releases`, #370): read from the Secrets Helm stores
//! each revision in. This module owns decoding a revision's payload
//! ([`decode`]), the table of a context's releases ([`table`]) and the watch
//! that fills it ([`watch`]); the panels and actions build on them.

mod decode;
mod table;
mod watch;

// UNWIRED(#370): the panels (helm-releases sections 3-4) are the first
// callers of the rest.
#[allow(unused_imports)]
pub(crate) use decode::{
    ChartMeta, DecodeError, HelmRelease, JsonErrorKind, ReleaseInfo, decode_release,
};
#[allow(unused_imports)]
pub(crate) use table::{Fetch, HelmReleasesTable, ReleaseKey, ReleaseRow, Revision, Summary};
pub(crate) use watch::{HelmScope, watch_helm_releases};
