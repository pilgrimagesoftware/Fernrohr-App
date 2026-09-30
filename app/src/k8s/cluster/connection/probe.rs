//! Builds a client from a resolved `Config` and probes it (one server-version
//! request), and `error_chain`, the shared "render an error plus its whole
//! `source()` chain" helper every failure path in this module uses.

use super::*;

/// Renders `error` together with its whole `source()` chain, outermost first,
/// joined with `": "`.
///
/// `kube::Error`'s own `Display` is only the outer variant label - a connect
/// failure renders as `ServiceError: client error (Connect)`, which names the
/// phase but discards the `io::Error` underneath it (refused, unreachable, the
/// address actually dialled). `#[source]` chains are how `kube` carries that
/// detail, so every user-facing failure reason goes through here rather than a
/// bare `to_string()`.
///
/// Cyclic chains are tolerated: a `HashSet` of the pointers already rendered
/// stops a self-referential `source()` from looping forever.
pub(crate) fn error_chain(error: &dyn std::error::Error) -> String {
    use std::collections::HashSet;

    let mut parts = vec![error.to_string()];
    let mut seen: HashSet<*const ()> = HashSet::new();
    seen.insert(error as *const dyn std::error::Error as *const ());

    let mut source = error.source();
    while let Some(current) = source {
        let key = current as *const dyn std::error::Error as *const ();
        if !seen.insert(key) {
            break;
        }
        parts.push(current.to_string());
        source = current.source();
    }
    parts.join(": ")
}

/// Builds a client from `config` and runs one probe request (the server
/// version endpoint). Every failure mode — an unreachable server, a TLS
/// error, or an exec credential plugin that fails to produce a token — maps
/// to `Failed(reason)` rather than panicking.
pub async fn probe(config: Config) -> ConnectionState {
    let client = match Client::try_from(config) {
        Ok(client) => client,
        Err(error) => return ConnectionState::Failed(error_chain(&error)),
    };
    match client.apiserver_version().await {
        Ok(_) => ConnectionState::Connected(client),
        Err(error) => ConnectionState::Failed(error_chain(&error)),
    }
}

#[cfg(test)]
mod tests;
