use gpui_kit::{App, AppContext as _, Context, Entity};
use kube::{Client, Config};

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Connecting,
    Connected,
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
        Ok(_) => ConnectionState::Connected,
        Err(error) => ConnectionState::Failed(error.to_string()),
    }
}

/// A GPUI entity exposing connection state for one cluster context, so a
/// view can observe it and re-render as the connection progresses.
pub struct ClusterConnection {
    pub state: ConnectionState,
}

impl ClusterConnection {
    /// Starts connecting in the background; the returned entity begins in
    /// `Connecting` and updates itself (and notifies observers) once the
    /// probe on the tokio runtime completes.
    pub fn connect(config: Config, cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
                let state = probe(config).await;
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

        assert_eq!(state, ConnectionState::Connected);
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
}
