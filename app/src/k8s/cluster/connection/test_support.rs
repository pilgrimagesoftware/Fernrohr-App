//! A minimal fake HTTP/1.1 responder and the `kube::Config` fixtures the probe
//! and connect-path tests share.

// Named imports rather than `use super::*`: `gpui_kit::*` next to `#[gpui_kit::test]`
// would shadow the built-in `#[test]` for these plain async helpers.
use kube::Config;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A minimal HTTP/1.1 responder: reads one request off `stream` and
/// writes back a fixed response, no framework required.
pub(super) async fn respond_once(mut stream: tokio::net::TcpStream, body: &str, status: &str) {
    let mut buf = [0u8; 1024];
    let _ = stream.read(&mut buf).await;
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

pub(super) async fn version_info_json() -> &'static str {
    r#"{"major":"1","minor":"31","gitVersion":"v1.31.0","gitCommit":"","gitTreeState":"","buildDate":"","goVersion":"","compiler":"","platform":""}"#
}

pub(super) fn config_for(addr: std::net::SocketAddr) -> Config {
    let mut config = Config::new(format!("http://{addr}").parse().unwrap());
    config.connect_timeout = Some(Duration::from_millis(500));
    config
}
