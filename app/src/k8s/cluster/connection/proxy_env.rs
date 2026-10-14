//! The proxy a client takes from the environment (#198). `kube` falls back to
//! `HTTPS_PROXY` / `https_proxy` when a cluster sets no `proxy-url`, but it
//! doesn't read `NO_PROXY` - so a cluster `kubectl` reaches directly, because
//! `NO_PROXY` names it, went through a proxy that may not reach it, and the
//! connect timed out. This applies `NO_PROXY` / `no_proxy` the way `kubectl`
//! does, to a proxy that came from the environment only: a cluster's own
//! `proxy-url` is the kubeconfig's explicit choice and is kept.

use kube::Config;
use kube::config::Kubeconfig;

/// The `proxy-url` the kubeconfig sets on `context`'s cluster (the current
/// context when `None`), if it sets one.
pub(super) fn kubeconfig_proxy(kubeconfig: &Kubeconfig, context: Option<&str>) -> Option<String> {
    let context = context.or(kubeconfig.current_context.as_deref())?;
    let cluster = kubeconfig
        .contexts
        .iter()
        .find(|named| named.name == context)?
        .context
        .as_ref()?
        .cluster
        .clone();
    kubeconfig
        .clusters
        .iter()
        .find(|named| named.name == cluster)?
        .cluster
        .as_ref()?
        .proxy_url
        .clone()
        .filter(|url| !url.is_empty())
}

/// Drops `config`'s proxy when it came from the environment (`own_proxy` is
/// the kubeconfig's own, which always stays) and `no_proxy` names the API
/// server's host.
pub(super) fn apply_no_proxy(config: &mut Config, own_proxy: bool, no_proxy: Option<&str>) {
    if own_proxy || config.proxy_url.is_none() {
        return;
    }
    let Some(host) = config.cluster_url.host() else {
        return;
    };
    if no_proxy.is_some_and(|list| bypasses(list, host)) {
        log::debug!("NO_PROXY names {host}: connecting to it without the environment's proxy");
        config.proxy_url = None;
    }
}

/// `proxy` as it may be shown or logged: `scheme://host:port` only. A proxy
/// URL can carry credentials (`http://user:pass@host:port`), which must reach
/// neither the UI nor the app log Report Issue attaches; a path or query is
/// dropped too, since a proxy needs neither.
pub(super) fn redacted(proxy: &http::Uri) -> String {
    let scheme = proxy.scheme_str().unwrap_or("http");
    let host = proxy.host().unwrap_or_default();
    match proxy.port_u16() {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    }
}

/// `NO_PROXY` / `no_proxy` from the environment, the upper case first, as
/// `kubectl` reads it.
pub(super) fn env_no_proxy() -> Option<String> {
    std::env::var("NO_PROXY")
        .ok()
        .or_else(|| std::env::var("no_proxy").ok())
        .filter(|list| !list.trim().is_empty())
}

/// Whether the comma-separated `list` names `host`: `*`, the host itself, or
/// a domain it's in - `example.com` and `.example.com` both cover
/// `api.example.com`. Ports on entries are ignored, as is case.
pub(super) fn bypasses(list: &str, host: &str) -> bool {
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    list.split(',')
        .map(|entry| entry.trim().to_ascii_lowercase())
        .filter(|entry| !entry.is_empty())
        .any(|entry| {
            if entry == "*" {
                return true;
            }
            let name = strip_port(&entry);
            let domain = name.trim_start_matches('.');
            host == domain || host.ends_with(&format!(".{domain}"))
        })
}

/// `entry` without a trailing `:port` - but an IPv6 address keeps its colons.
fn strip_port(entry: &str) -> &str {
    if let Some(rest) = entry.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    match entry.rsplit_once(':') {
        Some((name, port)) if !name.contains(':') && port.chars().all(|c| c.is_ascii_digit()) => {
            name
        }
        _ => entry,
    }
}

#[cfg(test)]
#[path = "proxy_env_tests.rs"]
mod tests;
