//! The connect path: resolving `Config` for a context (or its tunnel binding),
//! waiting for a bound tunnel's forward to come up, rewriting the config to
//! route through it, and `ClusterConnection::connect` itself, which wires all
//! of that into a GPUI entity. Probing a resolved `Config` is `probe.rs`'s.

use super::*;
use crate::k8s::cluster::oidc;

/// Resolves `Config` for `context_name`, or the kubeconfig's own `current-context`
/// (or in-cluster config) when `None`. Pure async, no GPUI context, so it's testable
/// on its own - the picker's "connect to this specific context" behavior lives here,
/// not scattered across [`ClusterConnection::connect`].
///
/// A context authenticating through an exec plugin gets the login shell's
/// `PATH` for it (#178, `exec_path`), so a Dock-launched app finds the plugin
/// as a terminal would; one authenticating through the `oidc` auth-provider
/// has an expired id-token renewed first (#188, `oidc`).
pub(in crate::k8s::cluster) async fn resolve_config(
    context_name: Option<&str>,
) -> Result<Config, String> {
    let config = match (context_name, Kubeconfig::read()) {
        (Some(name), kubeconfig) => {
            let kubeconfig = kubeconfig.map_err(|error| error_chain(&error))?;
            resolve_with_fresh_tokens(kubeconfig, Some(name), &oidc::kubeconfig_files()).await
        }
        // The current context, through the same renewal; `Config::infer` only
        // for what has no kubeconfig context to renew (in-cluster config).
        (None, Ok(kubeconfig)) if kubeconfig.current_context.is_some() => {
            resolve_with_fresh_tokens(kubeconfig, None, &oidc::kubeconfig_files()).await
        }
        (None, _) => Config::infer().await.map_err(|error| error_chain(&error)),
    }?;
    Ok(with_login_path(config).await)
}

/// `context` (the current one when `None`) resolved from `kubeconfig` after
/// renewing its user's OIDC id-token if that has expired, the renewed tokens
/// saved to whichever of `files` defines the user. Takes an already-loaded
/// [`Kubeconfig`] rather than reading `$KUBECONFIG`/`~/.kube/config` itself -
/// the seam that lets tests inject a fixture kubeconfig instead of this
/// machine's real one.
async fn resolve_with_fresh_tokens(
    mut kubeconfig: Kubeconfig,
    context: Option<&str>,
    files: &[std::path::PathBuf],
) -> Result<Config, String> {
    oidc::refresh_expired(&mut kubeconfig, context, files).await?;
    Config::from_custom_kubeconfig(
        kubeconfig,
        &KubeConfigOptions {
            context: context.map(str::to_string),
            ..Default::default()
        },
    )
    .await
    .map_err(|error| error_chain(&error))
}

/// `config` with its exec plugin, if any, pointed at the login shell's `PATH`.
/// Resolving that runs the login shell the first time - up to
/// `LOGIN_SHELL_TIMEOUT` - so it's done on a blocking thread, and only for a
/// config that has a plugin to find.
#[cfg(unix)]
async fn with_login_path(mut config: Config) -> Config {
    if config.auth_info.exec.is_none() {
        return config;
    }
    let path = tokio::task::spawn_blocking(crate::util::login_env::login_path)
        .await
        .map(str::to_string)
        .unwrap_or_default();
    if !path.is_empty() {
        crate::k8s::cluster::exec_path::use_login_path(&mut config, &path);
    }
    config
}

#[cfg(not(unix))]
async fn with_login_path(config: Config) -> Config {
    config
}

/// The context name whose tunnel binding decides `connect`'s forward, per section 1.2:
/// a picker-selected `context_name` always wins over the kubeconfig's own
/// current-context, which is only consulted when `context_name` is `None` (the
/// `None`-caller path, unchanged from before this section).
fn resolve_bound_context(context_name: Option<String>) -> Option<String> {
    context_name.or_else(|| {
        crate::k8s::cluster::kubeconfig::current_context_name(None)
            .ok()
            .flatten()
    })
}

/// What the connect path waits on and routes through for a tunnel-bound context:
/// the forward's state, its local address, and whether that address is the API
/// server or a proxy to it.
pub(in crate::k8s::cluster) struct ForwardWait {
    pub(in crate::k8s::cluster) state: watch::Receiver<ForwardState>,
    pub(in crate::k8s::cluster) local_addr: SocketAddr,
    pub(in crate::k8s::cluster) route: TunnelRoute,
}

impl ForwardWait {
    pub(in crate::k8s::cluster) fn of(handle: &RegistryHandle<ForwardKey, TunnelForward>) -> Self {
        let forward = handle.forward();
        Self {
            state: forward.state(),
            local_addr: forward.local_addr(),
            route: forward.route(),
        }
    }
}

/// Routes `config` through a tunnel's local port: rewritten to it for an SSH or
/// forward-mode tunnel, or through it as an HTTP proxy for a proxy-mode one - the real
/// API server URL and TLS name kept, so TLS runs end to end and is validated against
/// the real host. The proxy is set on this client only, never on the process
/// environment, so other contexts and the app's other requests don't use it.
fn route_through_tunnel(config: &mut Config, local_addr: SocketAddr, route: TunnelRoute) {
    match route {
        TunnelRoute::Rewrite => rewrite_for_tunnel(config, local_addr),
        TunnelRoute::Proxy => {
            if let Some(own) = &config.proxy_url {
                log::debug!("the bound tunnel's proxy overrides the kubeconfig's proxy-url {own}");
            }
            config.proxy_url = Some(
                format!("http://{local_addr}")
                    .parse()
                    .expect("a socket addr always parses as a URI authority"),
            );
        }
    }
}

/// A proxy-mode failure, saying it went through the tunnel's proxy - and, when the
/// proxy refused the `CONNECT` (hyper-util reports that only as "tunnel error"), that
/// the proxy is what refused.
fn name_the_proxy(state: ConnectionState, proxy: SocketAddr) -> ConnectionState {
    match state {
        ConnectionState::Failed(reason) if reason.contains("tunnel error") => {
            ConnectionState::Failed(format!(
                "the tunnel's proxy at {proxy} refused to connect to the API server ({reason})"
            ))
        }
        ConnectionState::Failed(reason) => {
            ConnectionState::Failed(format!("{reason} (through the tunnel's proxy at {proxy})"))
        }
        other => other,
    }
}

/// Points `config.cluster_url` at the tunnel's local forward and pins
/// `tls_server_name` to the API server host the certificate was actually issued for
/// (section 1.2's spike), unless the kubeconfig already set one explicitly.
fn rewrite_for_tunnel(config: &mut Config, local_addr: SocketAddr) {
    let original_host = config.cluster_url.host().unwrap_or_default().to_string();
    let scheme = config.cluster_url.scheme_str().unwrap_or("https");
    if config.tls_server_name.is_none() {
        config.tls_server_name = Some(original_host);
    }
    config.cluster_url = format!("{scheme}://{local_addr}")
        .parse()
        .expect("scheme + a socket addr always parse as a URI");
}

/// Section 6.2's connect-path core, factored out of [`ClusterConnection::connect`] so
/// it's testable without a GPUI context or a real kubeconfig: given an already-resolved
/// `Config` (or the error `Config::infer` produced) and, for a tunnel-bound context, the
/// forward's state receiver and local address, waits for the forward to reach `Up`
/// (reporting `WaitingForTunnel` meanwhile), rewrites the config, and probes - sending
/// every intermediate and final state to `tx` in order.
///
/// `pub(in crate::k8s::cluster)`: section 7.3's credential-refresh path (`cluster::session`)
/// reuses this directly rather than duplicating the tunnel-wait/rewrite/probe sequence -
/// a 401 needs exactly the same "resolve config, wait for the tunnel if bound, probe"
/// dance a first connect does, just triggered by a different signal.
pub(in crate::k8s::cluster) async fn connect_and_probe(
    config_result: Result<Config, String>,
    forward_wait: Option<ForwardWait>,
    tx: mpsc::Sender<ConnectionState>,
) {
    let rewrite = if let Some(ForwardWait {
        state: mut state_rx,
        local_addr,
        route,
    }) = forward_wait
    {
        let _ = tx.send(ConnectionState::WaitingForTunnel).await;
        loop {
            if *state_rx.borrow() == ForwardState::Up {
                break Some((local_addr, route));
            }
            if state_rx.changed().await.is_err() {
                let _ = tx
                    .send(ConnectionState::Failed(
                        "tunnel forward closed before becoming ready".to_string(),
                    ))
                    .await;
                return;
            }
        }
    } else {
        None
    };

    let state = match config_result {
        Ok(mut config) => {
            if let Some((local_addr, route)) = rewrite {
                route_through_tunnel(&mut config, local_addr, route);
            }
            let state = probe(config).await;
            match rewrite {
                Some((local_addr, TunnelRoute::Proxy)) => name_the_proxy(state, local_addr),
                _ => state,
            }
        }
        Err(error) => ConnectionState::Failed(error),
    };
    let _ = tx.send(state).await;
}

impl ClusterConnection {
    /// Starts connecting to `context_name`, or the kubeconfig's own current-context
    /// (or in-cluster config, if run inside a cluster) when `context_name` is `None`,
    /// in the background; the returned entity begins in `Connecting` and updates
    /// itself (and notifies observers) once the probe on the tokio runtime completes.
    ///
    /// If that context is bound to a tunnel (`tunnels.toml`), acquires the shared
    /// forward first, reports `WaitingForTunnel` until it reaches `Up`, then rewrites
    /// the resolved config to route through it before probing - section 6.2. Resolving
    /// the binding and acquiring the forward both happen synchronously here, on the
    /// GPUI foreground thread, since acquiring needs `&mut App`; only the already-
    /// extracted state receiver and local address move into the background task.
    ///
    /// Section 2.3: a bound context whose forward fails to acquire (its kubeconfig
    /// server URL has no host, most commonly) fails the connection outright with that
    /// reason - unlike an unbound context, there is no direct-connect fallback here,
    /// since silently ignoring a configured tunnel would route traffic somewhere the
    /// user didn't ask for. No background task is spawned in that case, so no `ssh`
    /// ever starts.
    pub fn connect(cx: &mut App, context_name: Option<String>) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            let bound_context = resolve_bound_context(context_name.clone());
            let tunnels_path = crate::util::paths::preference_dir().join("tunnels.toml");
            let acquired = bound_context
                .as_deref()
                .map(|context| tunnel::acquire_for_context(cx, &tunnels_path, None, context));

            if let Some(Err(error)) = acquired {
                return Self {
                    state: ConnectionState::Failed(error.to_string()),
                    since: Instant::now(),
                    _forward: None,
                };
            }
            let forward = acquired.and_then(Result::ok).flatten();
            let forward_wait = forward.as_ref().map(ForwardWait::of);

            let rx = crate::runtime::spawn_stream(cx, 4, move |tx| async move {
                let config_result = resolve_config(context_name.as_deref()).await;
                connect_and_probe(config_result, forward_wait, tx).await;
            });
            cx.spawn(async move |this, cx| {
                crate::runtime::drain(rx, |state| {
                    let _ = this.update(cx, |this, cx| {
                        this.state = state;
                        this.since = Instant::now();
                        cx.notify();
                    });
                })
                .await;
            })
            .detach();
            Self {
                state: ConnectionState::Connecting,
                since: Instant::now(),
                _forward: forward,
            }
        })
    }
}

#[cfg(test)]
mod oidc_tests;
#[cfg(test)]
mod proxy_tests;
#[cfg(test)]
mod tests;
