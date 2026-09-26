use super::tunnel;
use crate::forward_registry::RegistryHandle;
use crate::managed_forward::{ForwardState, ManagedForward as _};
use crate::ssh_tunnel::SshTunnel;
use gpui_kit::{App, AppContext as _, Context, Entity};
use kube::{Client, Config};
use std::net::SocketAddr;
use tokio::sync::{mpsc, watch};

#[derive(Clone)]
pub enum ConnectionState {
    Connecting,
    /// The context is bound to a tunnel whose forward hasn't reached `Up` yet. Distinct
    /// from `Connecting` so a view can show "waiting for tunnel" rather than a generic
    /// spinner - the wait here is for `ssh`, not for the API server itself.
    WaitingForTunnel,
    Connected(Client),
    Failed(String),
}

/// Builds a client from `config` and runs one probe request (the server
/// version endpoint). Every failure mode — an unreachable server, a TLS
/// error, or an exec credential plugin that fails to produce a token — maps
/// to `Failed(reason)` rather than panicking.
pub async fn probe(config: Config) -> ConnectionState {
    let client = match Client::try_from(config) {
        Ok(client) => client,
        Err(error) => return ConnectionState::Failed(error.to_string()),
    };
    match client.apiserver_version().await {
        Ok(_) => ConnectionState::Connected(client),
        Err(error) => ConnectionState::Failed(error.to_string()),
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
async fn connect_and_probe(
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

/// A GPUI entity exposing connection state for one cluster context, so a
/// view can observe it and re-render as the connection progresses.
pub struct ClusterConnection {
    pub state: ConnectionState,
    /// Kept alive for as long as this connection exists - section 6.2's "release the
    /// forward when the last session using it disconnects" is just this field's own
    /// `Drop` (via `RegistryHandle`/`SshTunnel`'s), since there is one `ClusterSession`
    /// (and so one `ClusterConnection`) per app today. `None` for an unbound context.
    _forward: Option<RegistryHandle<SshTunnel>>,
}

impl ClusterConnection {
    /// The bound forward's state receiver, for section 7.2's `ConnectionHealth` to watch -
    /// `None` for an unbound context, which has no forward to go unhealthy.
    pub fn forward_state(&self) -> Option<watch::Receiver<ForwardState>> {
        self._forward
            .as_ref()
            .map(|handle| handle.forward().state())
    }

    /// Starts connecting to the context selected by `$KUBECONFIG`/
    /// `~/.kube/config`'s current-context (or in-cluster config, if run
    /// inside a cluster) in the background; the returned entity begins in
    /// `Connecting` and updates itself (and notifies observers) once the
    /// probe on the tokio runtime completes.
    ///
    /// If that context is bound to a tunnel (`tunnels.toml`), acquires the shared
    /// forward first, reports `WaitingForTunnel` until it reaches `Up`, then rewrites
    /// the inferred config to route through it before probing - section 6.2. Resolving
    /// the binding and acquiring the forward both happen synchronously here, on the
    /// GPUI foreground thread, since acquiring needs `&mut App`; only the already-
    /// extracted state receiver and local address move into the background task.
    pub fn connect(cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            let context_name = crate::cluster::kubeconfig::current_context_name(None)
                .ok()
                .flatten();
            let tunnels_path = crate::paths::preference_dir().join("tunnels.toml");
            let forward = context_name.and_then(|context| {
                tunnel::acquire_for_context(cx, &tunnels_path, &context)
                    .ok()
                    .flatten()
            });
            let forward_wait = forward
                .as_ref()
                .map(|handle| (handle.forward().state(), handle.forward().local_addr()));

            let rx = crate::runtime::spawn_stream(cx, 4, move |tx| async move {
                let config_result = Config::infer().await.map_err(|error| error.to_string());
                connect_and_probe(config_result, forward_wait, tx).await;
            });
            cx.spawn(async move |this, cx| {
                crate::runtime::drain(rx, |state| {
                    let _ = this.update(cx, |this, cx| {
                        this.state = state;
                        cx.notify();
                    });
                })
                .await;
            })
            .detach();
            Self {
                state: ConnectionState::Connecting,
                _forward: forward,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kube::config::{AuthInfo, ExecConfig};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// A minimal HTTP/1.1 responder: reads one request off `stream` and
    /// writes back a fixed response, no framework required.
    async fn respond_once(mut stream: tokio::net::TcpStream, body: &str, status: &str) {
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf).await;
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
    }

    async fn version_info_json() -> &'static str {
        r#"{"major":"1","minor":"31","gitVersion":"v1.31.0","gitCommit":"","gitTreeState":"","buildDate":"","goVersion":"","compiler":"","platform":""}"#
    }

    fn config_for(addr: std::net::SocketAddr) -> Config {
        let mut config = Config::new(format!("http://{addr}").parse().unwrap());
        config.connect_timeout = Some(Duration::from_millis(500));
        config
    }

    #[tokio::test]
    async fn probe_succeeds_against_a_reachable_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            respond_once(stream, version_info_json().await, "200 OK").await;
        });

        let state = probe(config_for(addr)).await;

        assert!(matches!(state, ConnectionState::Connected(_)));
    }

    #[tokio::test]
    async fn probe_fails_against_an_unreachable_server() {
        // Bind then immediately drop: the port is free but nothing answers,
        // giving a real, fast connection-refused rather than a hang.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let state = probe(config_for(addr)).await;

        assert!(matches!(state, ConnectionState::Failed(_)));
    }

    #[tokio::test]
    async fn probe_fails_when_the_exec_plugin_fails() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // No server is needed: the exec plugin fails before any request is sent.
        drop(listener);

        let mut config = config_for(addr);
        config.auth_info = AuthInfo {
            exec: Some(ExecConfig {
                api_version: Some("client.authentication.k8s.io/v1".into()),
                command: Some("fernrohr-test-nonexistent-credential-plugin".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let state = probe(config).await;

        assert!(matches!(state, ConnectionState::Failed(_)));
    }

    /// Section 6.2: `connect_and_probe`'s wait/success/fail behavior for a tunnel-bound
    /// context, driven directly (no GPUI, no real kubeconfig) against a fake watch
    /// channel standing in for a `ManagedForward`'s state and a fake HTTP server
    /// standing in for the API server - the same seam `ForwardSupervisor`'s own tests
    /// (section 2.3) drive, plugged in here one layer up.
    mod connect_and_probe_tests {
        use super::*;
        use crate::managed_forward::ForwardState;

        async fn recv_all(mut rx: mpsc::Receiver<ConnectionState>) -> Vec<&'static str> {
            let mut labels = Vec::new();
            while let Some(state) = rx.recv().await {
                labels.push(match state {
                    ConnectionState::Connecting => "Connecting",
                    ConnectionState::WaitingForTunnel => "WaitingForTunnel",
                    ConnectionState::Connected(_) => "Connected",
                    ConnectionState::Failed(_) => "Failed",
                });
            }
            labels
        }

        #[tokio::test]
        async fn waits_then_succeeds_once_the_forward_reaches_up() {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                respond_once(stream, version_info_json().await, "200 OK").await;
            });

            let (state_tx, state_rx) = watch::channel(ForwardState::Connecting);
            let (tx, rx) = mpsc::channel(4);
            let handle = tokio::spawn(connect_and_probe(
                Ok(config_for(addr)),
                Some((state_rx, addr)),
                tx,
            ));

            // Give connect_and_probe a moment to report the wait before releasing it.
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            state_tx.send(ForwardState::Up).unwrap();
            handle.await.unwrap();

            assert_eq!(recv_all(rx).await, vec!["WaitingForTunnel", "Connected"]);
        }

        #[tokio::test]
        async fn stays_waiting_when_the_tunnel_never_comes_up() {
            let (_state_tx, state_rx) = watch::channel(ForwardState::Reconnecting);
            let (tx, mut rx) = mpsc::channel(4);
            // _state_tx is kept alive (never dropped) so state_rx.changed() blocks
            // rather than erroring - the "sustained failure" case from forward_supervisor's
            // own tests, one layer up: the forward keeps retrying, never reaching Up.
            let _task = tokio::spawn(connect_and_probe(
                Ok(config_for("127.0.0.1:1".parse().unwrap())),
                Some((state_rx, "127.0.0.1:1".parse().unwrap())),
                tx,
            ));

            let first = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
                .await
                .expect("should report WaitingForTunnel promptly")
                .unwrap();
            assert!(matches!(first, ConnectionState::WaitingForTunnel));

            let second =
                tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await;
            assert!(
                second.is_err(),
                "must not report Connected/Failed while the forward is still retrying"
            );
        }

        #[tokio::test]
        async fn unbound_context_probes_directly_with_no_wait() {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                respond_once(stream, version_info_json().await, "200 OK").await;
            });

            let (tx, rx) = mpsc::channel(4);
            connect_and_probe(Ok(config_for(addr)), None, tx).await;

            assert_eq!(recv_all(rx).await, vec!["Connected"]);
        }

        #[test]
        fn rewrite_pins_tls_server_name_to_the_original_host_and_points_at_local_addr() {
            let mut config = Config::new("https://api.example.com:6443".parse().unwrap());
            let local_addr: std::net::SocketAddr = "127.0.0.1:54321".parse().unwrap();

            rewrite_for_tunnel(&mut config, local_addr);

            assert_eq!(config.cluster_url.to_string(), "https://127.0.0.1:54321/");
            assert_eq!(config.tls_server_name.as_deref(), Some("api.example.com"));
        }

        #[test]
        fn rewrite_does_not_override_an_explicit_tls_server_name() {
            let mut config = Config::new("https://api.example.com:6443".parse().unwrap());
            config.tls_server_name = Some("already-pinned.internal".to_string());
            let local_addr: std::net::SocketAddr = "127.0.0.1:54321".parse().unwrap();

            rewrite_for_tunnel(&mut config, local_addr);

            assert_eq!(
                config.tls_server_name.as_deref(),
                Some("already-pinned.internal")
            );
        }
    }

    /// Section 1.2 spike: through an SSH tunnel, `cluster_url` points at
    /// `127.0.0.1:<local-port>` but the certificate is issued for the real API
    /// server host, so `tls_server_name` must be pinned to that host or
    /// validation fails. Proven here with a self-signed cert (rather than a
    /// real bastioned cluster) whose SAN deliberately excludes "127.0.0.1".
    mod tls_rewrite {
        use super::*;
        use rcgen::{CertifiedKey, generate_simple_self_signed};
        use rustls::ServerConfig;
        use rustls::pki_types::{CertificateDer, PrivateKeyDer};
        use std::sync::Once;
        use tokio_rustls::TlsAcceptor;

        const PINNED_HOST: &str = "fernrohr-tls-spike.internal";

        fn install_crypto_provider() {
            static ONCE: Once = Once::new();
            ONCE.call_once(|| {
                let _ = rustls::crypto::ring::default_provider().install_default();
            });
        }

        fn self_signed_cert() -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
            let CertifiedKey { cert, signing_key } =
                generate_simple_self_signed(vec![PINNED_HOST.to_string()]).unwrap();
            let key = PrivateKeyDer::Pkcs8(signing_key.into());
            (cert.der().clone(), key)
        }

        /// Serves one HTTP/1.1 response over TLS using `cert`/`key`, returning the
        /// address to connect to and the cert's DER bytes to trust as root CA.
        async fn spawn_tls_server() -> (std::net::SocketAddr, Vec<u8>) {
            install_crypto_provider();
            let (cert, key) = self_signed_cert();
            let root_cert_der = cert.to_vec();

            let server_config = ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![cert], key)
                .unwrap();
            let acceptor = TlsAcceptor::from(std::sync::Arc::new(server_config));

            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let tls_stream = acceptor.accept(stream).await.unwrap();
                let (mut reader, mut writer) = tokio::io::split(tls_stream);
                let mut buf = [0u8; 1024];
                let _ = reader.read(&mut buf).await;
                let body = version_info_json().await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = writer.write_all(response.as_bytes()).await;
                let _ = writer.shutdown().await;
            });
            (addr, root_cert_der)
        }

        #[tokio::test]
        async fn probe_succeeds_when_tls_server_name_is_pinned_to_the_real_host() {
            let (addr, root_cert_der) = spawn_tls_server().await;

            let mut config = Config::new(format!("https://{addr}").parse().unwrap());
            config.connect_timeout = Some(Duration::from_millis(500));
            config.root_cert = Some(vec![root_cert_der]);
            config.tls_server_name = Some(PINNED_HOST.to_string());

            let state = probe(config).await;

            assert!(
                matches!(state, ConnectionState::Connected(_)),
                "expected the pinned server name to validate against the cert's SAN"
            );
        }

        #[tokio::test]
        async fn probe_fails_without_the_pin_because_127_0_0_1_is_not_in_the_cert() {
            let (addr, root_cert_der) = spawn_tls_server().await;

            // No `tls_server_name`: kube falls back to validating against the
            // `cluster_url` host, "127.0.0.1", which the cert was never issued for.
            let mut config = Config::new(format!("https://{addr}").parse().unwrap());
            config.connect_timeout = Some(Duration::from_millis(500));
            config.root_cert = Some(vec![root_cert_der]);

            let state = probe(config).await;

            assert!(
                matches!(state, ConnectionState::Failed(_)),
                "expected certificate validation to fail without the server-name pin"
            );
        }
    }
}
