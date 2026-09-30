//! The connect path: resolving `Config` for a context (or its tunnel binding),
//! waiting for a bound tunnel's forward to come up, rewriting the config to
//! route through it, and `ClusterConnection::connect` itself, which wires all
//! of that into a GPUI entity. Probing a resolved `Config` is `probe.rs`'s.

use super::*;

/// Resolves `Config` for `context_name`, or the kubeconfig's own `current-context`
/// (or in-cluster config) when `None`. Pure async, no GPUI context, so it's testable
/// on its own - the picker's "connect to this specific context" behavior lives here,
/// not scattered across [`ClusterConnection::connect`].
pub(in crate::k8s::cluster) async fn resolve_config(
    context_name: Option<&str>,
) -> Result<Config, String> {
    match context_name {
        Some(name) => {
            let kubeconfig = Kubeconfig::read().map_err(|error| error_chain(&error))?;
            resolve_named_context(kubeconfig, name).await
        }
        None => Config::infer().await.map_err(|error| error_chain(&error)),
    }
}

/// [`resolve_config`]'s named-context branch, taking an already-loaded [`Kubeconfig`]
/// rather than reading `$KUBECONFIG`/`~/.kube/config` itself - the seam that lets tests
/// inject a fixture kubeconfig instead of this machine's real one.
async fn resolve_named_context(
    kubeconfig: Kubeconfig,
    context_name: &str,
) -> Result<Config, String> {
    Config::from_custom_kubeconfig(
        kubeconfig,
        &KubeConfigOptions {
            context: Some(context_name.to_string()),
            ..Default::default()
        },
    )
    .await
    .map_err(|error| error_chain(&error))
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
    forward_wait: Option<(watch::Receiver<ForwardState>, SocketAddr)>,
    tx: mpsc::Sender<ConnectionState>,
) {
    let rewrite = if let Some((mut state_rx, local_addr)) = forward_wait {
        let _ = tx.send(ConnectionState::WaitingForTunnel).await;
        loop {
            if *state_rx.borrow() == ForwardState::Up {
                break Some(local_addr);
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
            if let Some(local_addr) = rewrite {
                rewrite_for_tunnel(&mut config, local_addr);
            }
            probe(config).await
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
            let forward_wait = forward
                .as_ref()
                .map(|handle| (handle.forward().state(), handle.forward().local_addr()));

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
mod tests;
