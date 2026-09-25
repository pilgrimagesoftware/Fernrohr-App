use gpui_kit::{App, AppContext as _, Context, Entity};
use kube::{Client, Config};

#[derive(Clone)]
pub enum ConnectionState {
    Connecting,
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

/// A GPUI entity exposing connection state for one cluster context, so a
/// view can observe it and re-render as the connection progresses.
pub struct ClusterConnection {
    pub state: ConnectionState,
}

impl ClusterConnection {
    /// Starts connecting to the context selected by `$KUBECONFIG`/
    /// `~/.kube/config`'s current-context (or in-cluster config, if run
    /// inside a cluster) in the background; the returned entity begins in
    /// `Connecting` and updates itself (and notifies observers) once the
    /// probe on the tokio runtime completes.
    pub fn connect(cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
                let state = match Config::infer().await {
                    Ok(config) => probe(config).await,
                    Err(error) => ConnectionState::Failed(error.to_string()),
                };
                let _ = tx.send(state).await;
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
