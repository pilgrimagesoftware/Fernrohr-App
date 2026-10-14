//! Saying what a connect timeout means (#198). A connect that never completes,
//! because the API server's address is unreachable without a VPN or bastion or
//! a proxy can't reach it, ends as `kube`'s nested `ServiceError: client error
//! (Connect): ... deadline has elapsed`, which names neither the server nor
//! what to check. This rewrites such a failure to name what timed out,
//! how long it waited, and what to look at, keeping the original error after.

use std::time::Duration;

/// What a connection attempt was dialling, for its timeout message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Dialled {
    /// The API server's `host:port` - or, for a tunnel-rewritten client, the
    /// tunnel's local port.
    pub(super) endpoint: String,
    /// The HTTP proxy the client went through, if any.
    pub(super) proxy: Option<String>,
    /// Whether `endpoint` is a tunnel's local port rather than the server.
    pub(super) through_tunnel: bool,
    /// The client's connect timeout.
    pub(super) timeout: Option<Duration>,
}

impl Dialled {
    /// What `config` is about to dial.
    pub(super) fn of(config: &kube::Config, through_tunnel: bool) -> Self {
        let url = &config.cluster_url;
        let host = url.host().unwrap_or_default();
        let port = url.port_u16().unwrap_or(match url.scheme_str() {
            Some("http") => 80,
            _ => 443,
        });
        Self {
            endpoint: format!("{host}:{port}"),
            proxy: config.proxy_url.as_ref().map(ToString::to_string),
            through_tunnel,
            timeout: config.connect_timeout,
        }
    }
}

/// Whether `reason` is a connect that timed out: the client's own connect
/// deadline, or the OS giving up on the TCP handshake.
pub(super) fn is_connect_timeout(reason: &str) -> bool {
    let reason = reason.to_ascii_lowercase();
    reason.contains("deadline has elapsed")
        || reason.contains("timed out")
        || reason.contains("timedout")
}

/// `reason`, explained when it's a connect timeout; unchanged otherwise.
pub(super) fn explain(reason: String, dialled: &Dialled) -> String {
    if !is_connect_timeout(&reason) {
        return reason;
    }
    let after = dialled
        .timeout
        .map(|timeout| match timeout.as_secs() {
            0 => format!(" after {}ms", timeout.as_millis()),
            secs => format!(" after {secs}s"),
        })
        .unwrap_or_default();
    let target = if dialled.through_tunnel {
        format!("the tunnel's local port {}", dialled.endpoint)
    } else {
        format!("the API server at {}", dialled.endpoint)
    };
    let via = dialled
        .proxy
        .as_ref()
        .map(|proxy| format!(" through the proxy {}", proxy.trim_end_matches('/')))
        .unwrap_or_default();
    let check = match (&dialled.proxy, dialled.through_tunnel) {
        (Some(_), _) => {
            "Check that the proxy is running and can reach the cluster - for a proxy from \
             HTTPS_PROXY, whether NO_PROXY should name the cluster instead"
        }
        (None, true) => "Check that the tunnel is up and its remote end can reach the cluster",
        (None, false) => {
            "Check that this machine can reach that address: any VPN, tunnel or bastion the \
             cluster needs is connected, and the kubeconfig's server address is right"
        }
    };
    format!("Timed out{after} connecting to {target}{via}. {check}. ({reason})")
}

#[cfg(test)]
#[path = "timeout_tests.rs"]
mod tests;
