//! Helm releases (`helm-releases`, #370): read from the Secrets Helm stores
//! each revision in. This module owns decoding a revision's payload
//! ([`decode`]); the watch, panels and actions build on it.

mod decode;

// UNWIRED(#370): the release watch (helm-releases section 2) is the first
// caller.
#[allow(unused_imports)]
pub(crate) use decode::{
    ChartMeta, DecodeError, HelmRelease, JsonErrorKind, ReleaseInfo, decode_release,
};
