//! Sections 4.1 and 4.2's test coverage. `kind`/Docker aren't available in this dev
//! environment (see section 3's local-`sshd` tests for the same constraint), so these
//! stand up a minimal fake API server instead: plain HTTP/1.1 over a real local
//! `TcpListener`, hand-rolled far enough to serve a `Pod` GET and to perform the
//! WebSocket upgrade `kube`'s portforward subresource expects, then speak its binary
//! channel-multiplexed wire protocol (see `kube_client::api::portforward`) well enough
//! to echo whatever the "forwarded port" receives - standing in for a real Service.
//!
//! Section 4.2 (target loss) needs no new production code: `PodPortForwardTransport`'s
//! `connect`/`health_check` already return `Err` on a 404 same as a non-`Running`
//! phase, and section 2.3's `ForwardSupervisor` already treats any `health_check` `Err`
//! as a transition to `Reconnecting` that retries `connect`. So the fake server's `Pod`
//! phase is made mutable here (`Arc<parking_lot::Mutex<PodPhase>>`) and
//! `pod_disappears_and_reappears_drives_the_supervisor_through_reconnecting` drives that
//! existing composition end to end rather than adding a new code path.

use super::*;
use crate::forward_supervisor::{BackoffPolicy, ForwardSupervisor, SupervisorOptions};
use crate::managed_forward::ForwardState;
use futures_util::{SinkExt, StreamExt};
use kube::{Client, Config};
use parking_lot::Mutex;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener as StdTcpListener;
use tokio::runtime::Handle;
use tokio::sync::watch;
use tokio::time::{Duration, timeout};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::Role;

const TEST_TIMEOUT: Duration = Duration::from_secs(5);
const REMOTE_PORT: u16 = 8080;

/// The fake API server's current answer for the `Pod` GET. `Deleted` answers 404 with a
/// minimal Kubernetes `Status` body, the same shape a real deleted Pod produces.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PodPhase {
    Running,
    Pending,
    Deleted,
}

impl PodPhase {
    fn as_str(self) -> &'static str {
        match self {
            PodPhase::Running => "Running",
            PodPhase::Pending => "Pending",
            PodPhase::Deleted => unreachable!("Deleted has no phase string - it's a 404"),
        }
    }
}

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

async fn respond_pod(stream: &mut TcpStream, phase: PodPhase) {
    let (status_line, body) = if phase == PodPhase::Deleted {
        (
            "HTTP/1.1 404 Not Found",
            r#"{"kind":"Status","apiVersion":"v1","status":"Failure","message":"pods \"target\" not found","reason":"NotFound","code":404}"#
                .to_string(),
        )
    } else {
        (
            "HTTP/1.1 200 OK",
            format!(
                r#"{{"apiVersion":"v1","kind":"Pod","metadata":{{"name":"target"}},"status":{{"phase":"{}"}}}}"#,
                phase.as_str()
            ),
        )
    };
    let response = format!(
        "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
/// the shared `phase`'s current value at request time - callers can flip it mid-test
/// (section 4.2) to simulate the Pod being deleted and later recreated.
async fn spawn_fake_api_server(
    initial: PodPhase,
) -> (
    SocketAddr,
    Arc<Mutex<PodPhase>>,
    tokio::task::JoinHandle<()>,
) {
    let listener = StdTcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let phase = Arc::new(Mutex::new(initial));
    let server_phase = phase.clone();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let phase = server_phase.clone();
            tokio::spawn(async move {
                let headers = read_request_headers(&mut stream).await;
                if extract_header(&headers, "upgrade").is_some_and(|v| v == "websocket") {
                    serve_portforward_upgrade(stream, &headers).await;
                } else {
                    let current_phase = *phase.lock();
                    respond_pod(&mut stream, current_phase).await;
                }
            });
        }
    });
    (addr, phase, handle)
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
    let (addr, _phase, _server) = spawn_fake_api_server(PodPhase::Running).await;
    let mut transport = PodPortForwardTransport::new(test_config(addr));

    assert!(transport.connect().await.is_ok());
}

#[tokio::test]
async fn connect_fails_when_pod_is_not_running() {
    let (addr, _phase, _server) = spawn_fake_api_server(PodPhase::Pending).await;
    let mut transport = PodPortForwardTransport::new(test_config(addr));

    let result = transport.connect().await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Pending"));
}

#[tokio::test]
async fn connect_fails_when_pod_is_deleted() {
    let (addr, _phase, _server) = spawn_fake_api_server(PodPhase::Deleted).await;
    let mut transport = PodPortForwardTransport::new(test_config(addr));

    assert!(transport.connect().await.is_err());
}

/// Section 4.1's required proof: a local `TcpListener` bridging accepted connections to
/// a fresh port-forward stream each, actually carrying bytes end to end.
#[tokio::test]
async fn serve_bridges_a_local_connection_to_the_pod_port_forward() {
    let (api_addr, _phase, _server) = spawn_fake_api_server(PodPhase::Running).await;
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

fn fast_options() -> SupervisorOptions {
    SupervisorOptions {
        health_check_interval: Duration::from_millis(5),
        backoff: BackoffPolicy {
            initial: Duration::from_millis(5),
            max: Duration::from_millis(20),
        },
    }
}

async fn wait_for(state: &mut watch::Receiver<ForwardState>, target: ForwardState) {
    timeout(TEST_TIMEOUT, async {
        loop {
            if *state.borrow() == target {
                return;
            }
            state.changed().await.unwrap();
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {target:?}"));
}

/// Section 4.2's required proof: deleting the backing Pod moves the forward to
/// `Reconnecting`, and it recovers to `Up` once a matching Pod returns. No new
/// production code drives this - see the module doc comment - so this test exercises
/// `ForwardSupervisor` (section 2.3) with `PodPortForwardTransport` the same way
/// `forward_supervisor`'s own tests exercise it with a scripted fake.
#[tokio::test]
async fn pod_disappears_and_reappears_drives_the_supervisor_through_reconnecting() {
    let (api_addr, phase, _server) = spawn_fake_api_server(PodPhase::Running).await;
    let transport = PodPortForwardTransport::new(test_config(api_addr));
    let local_addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
    let supervisor =
        ForwardSupervisor::spawn(&Handle::current(), local_addr, transport, fast_options());
    let mut state = supervisor.state();

    wait_for(&mut state, ForwardState::Up).await;

    *phase.lock() = PodPhase::Deleted;
    wait_for(&mut state, ForwardState::Reconnecting).await;

    *phase.lock() = PodPhase::Running;
    wait_for(&mut state, ForwardState::Up).await;
}
