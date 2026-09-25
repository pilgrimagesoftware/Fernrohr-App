//! Section 4.1's test coverage. `kind`/Docker aren't available in this dev
//! environment (see section 3's local-`sshd` tests for the same constraint), so these
//! stand up a minimal fake API server instead: plain HTTP/1.1 over a real local
//! `TcpListener`, hand-rolled far enough to serve a `Pod` GET and to perform the
//! WebSocket upgrade `kube`'s portforward subresource expects, then speak its binary
//! channel-multiplexed wire protocol (see `kube_client::api::portforward`) well enough
//! to echo whatever the "forwarded port" receives - standing in for a real Service.
//!
//! Not coverage of target-loss recovery (a Pod disappearing mid-forward) - that's
//! section 4.2.

use super::*;
use futures_util::{SinkExt, StreamExt};
use kube::{Client, Config};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener as StdTcpListener;
use tokio::time::{Duration, timeout};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::Role;

const TEST_TIMEOUT: Duration = Duration::from_secs(5);
const REMOTE_PORT: u16 = 8080;

/// Reads one HTTP/1.1 request's headers off `stream` (no body handling - every request
/// this fake server receives, `Pod` GET or portforward upgrade, is bodyless). Case is
/// preserved: `Sec-WebSocket-Key`'s value is base64 and must round-trip exactly, so
/// header *names* are matched case-insensitively by `extract_header` instead.
async fn read_request_headers(stream: &mut TcpStream) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let n = stream.read(&mut chunk).await.expect("read request");
        assert!(n > 0, "connection closed before headers completed");
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn extract_header<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    headers.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

async fn respond_pod_json(stream: &mut TcpStream, phase: &str) {
    let body = format!(
        r#"{{"apiVersion":"v1","kind":"Pod","metadata":{{"name":"target"}},"status":{{"phase":"{phase}"}}}}"#
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// Upgrades `stream` to a server-side WebSocket per the headers already read into
/// `headers`, then plays the portforward wire protocol for one port: sends the
/// required per-channel init frame (data channel 0, error channel 1, each a 2-byte
/// little-endian port number prefixed with the channel byte - see
/// `kube_client::api::portforward::forwarder_loop`) and echoes back whatever arrives on
/// the data channel, standing in for a real forwarded service.
async fn serve_portforward_upgrade(stream: TcpStream, headers: &str) {
    let key = extract_header(headers, "sec-websocket-key").expect("Sec-WebSocket-Key header");
    let accept = derive_accept_key(key.as_bytes());
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\nSec-WebSocket-Protocol: v4.channel.k8s.io\r\n\r\n"
    );
    let mut stream = stream;
    stream
        .write_all(response.as_bytes())
        .await
        .expect("write upgrade response");

    let mut ws = WebSocketStream::from_raw_socket(stream, Role::Server, None).await;

    let port_bytes = REMOTE_PORT.to_le_bytes();
    for channel in [0u8, 1u8] {
        let mut frame = vec![channel];
        frame.extend_from_slice(&port_bytes);
        ws.send(Message::Binary(frame.into()))
            .await
            .expect("send channel init frame");
    }

    while let Some(message) = ws.next().await {
        match message.expect("websocket message") {
            Message::Binary(bytes) if !bytes.is_empty() && bytes[0] == 0 => {
                let mut echo = vec![0u8];
                echo.extend_from_slice(&bytes[1..]);
                ws.send(Message::Binary(echo.into()))
                    .await
                    .expect("echo data channel");
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}

/// One fake API server: any request with WebSocket upgrade headers is treated as the
/// portforward call and echoes; anything else is answered as a `Pod` GET reporting
/// `phase`.
async fn spawn_fake_api_server(phase: &'static str) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = StdTcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let phase = phase;
            tokio::spawn(async move {
                let headers = read_request_headers(&mut stream).await;
                if extract_header(&headers, "upgrade").is_some_and(|v| v == "websocket") {
                    serve_portforward_upgrade(stream, &headers).await;
                } else {
                    respond_pod_json(&mut stream, phase).await;
                }
            });
        }
    });
    (addr, handle)
}

fn test_client(addr: SocketAddr) -> Client {
    let config = Config::new(format!("http://{addr}").parse().unwrap());
    Client::try_from(config).expect("build client")
}

fn test_config(addr: SocketAddr) -> K8sPortForwardConfig {
    K8sPortForwardConfig {
        client: test_client(addr),
        namespace: "ns".to_string(),
        pod_name: "target".to_string(),
        remote_port: REMOTE_PORT,
    }
}

#[tokio::test]
async fn connect_succeeds_when_pod_is_running() {
    let (addr, _server) = spawn_fake_api_server("Running").await;
    let mut transport = PodPortForwardTransport::new(test_config(addr));

    assert!(transport.connect().await.is_ok());
}

#[tokio::test]
async fn connect_fails_when_pod_is_not_running() {
    let (addr, _server) = spawn_fake_api_server("Pending").await;
    let mut transport = PodPortForwardTransport::new(test_config(addr));

    let result = transport.connect().await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Pending"));
}

/// Section 4.1's required proof: a local `TcpListener` bridging accepted connections to
/// a fresh port-forward stream each, actually carrying bytes end to end.
#[tokio::test]
async fn serve_bridges_a_local_connection_to_the_pod_port_forward() {
    let (api_addr, _server) = spawn_fake_api_server("Running").await;
    let config = test_config(api_addr);

    let local_listener = StdTcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = local_listener.local_addr().unwrap();
    let serve_task = tokio::spawn(super::serve(local_listener, config));

    let mut client = timeout(TEST_TIMEOUT, TcpStream::connect(local_addr))
        .await
        .expect("connect timed out")
        .expect("connect to local forward");

    client.write_all(b"hello from the client").await.unwrap();

    let mut buf = [0u8; 64];
    let n = timeout(TEST_TIMEOUT, client.read(&mut buf))
        .await
        .expect("echo timed out")
        .expect("read echo");

    assert_eq!(&buf[..n], b"hello from the client");

    serve_task.abort();
}
