//! `command-tunnels` 3.3: proxy mode against a real (in-process) HTTP `CONNECT` proxy
//! standing in for the tunnel, and a stub HTTPS API server whose certificate names
//! only its own host. A proxy-mode client reaches the stub through the proxy with TLS
//! validated end to end against that host; a client with no tunnel never touches the
//! proxy; and a proxy that refuses the API server fails the connection outright.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::connection::connect::{name_the_proxy, route_through_tunnel};
use crate::k8s::cluster::connection::probe::probe;
use crate::k8s::cluster::connection::test_support::version_info_json;
use crate::k8s::cluster::tunnel::TunnelRoute;
use kube::Config;
use parking_lot::Mutex;
use rcgen::{CertifiedKey, generate_simple_self_signed};
use rustls::ServerConfig;
use rustls::pki_types::PrivateKeyDer;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

/// The stub API server's name: in its certificate, and resolvable only by the proxy.
const API_HOST: &str = "api.fernrohr.test";

/// An HTTPS stub answering `/version`, trusted through the returned root cert, and
/// counting the connections it accepts.
async fn spawn_api_server() -> (u16, Vec<u8>, Arc<AtomicUsize>) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let CertifiedKey { cert, signing_key } =
        generate_simple_self_signed(vec![API_HOST.to_string()]).unwrap();
    let root = cert.der().to_vec();
    let server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.der().clone()],
            PrivateKeyDer::Pkcs8(signing_key.into()),
        )
        .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(server_config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let accepted = Arc::new(AtomicUsize::new(0));
    let counter = accepted.clone();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            counter.fetch_add(1, Ordering::SeqCst);
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(stream).await else {
                    return;
                };
                let mut buf = [0u8; 2048];
                let _ = tls.read(&mut buf).await;
                let body = version_info_json().await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = tls.write_all(response.as_bytes()).await;
                let _ = tls.shutdown().await;
            });
        }
    });
    (port, root, accepted)
}

/// A `CONNECT` proxy that tunnels to `API_HOST` (as `127.0.0.1`) and refuses any
/// other host with `403`, recording every target it is asked for.
async fn spawn_proxy() -> (SocketAddr, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let asked = Arc::new(Mutex::new(Vec::new()));
    let record = asked.clone();
    tokio::spawn(async move {
        loop {
            let (mut client, _) = listener.accept().await.unwrap();
            let record = record.clone();
            tokio::spawn(async move {
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    if client.read(&mut byte).await.unwrap_or(0) == 0 {
                        return;
                    }
                    head.push(byte[0]);
                }
                let head = String::from_utf8_lossy(&head);
                let target = head
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                record.lock().push(target.clone());
                let (host, port) = target.rsplit_once(':').unwrap_or_default();
                if host != API_HOST {
                    let _ = client.write_all(b"HTTP/1.1 403 Forbidden\r\n\r\n").await;
                    return;
                }
                let Ok(mut upstream) =
                    TcpStream::connect(("127.0.0.1", port.parse().unwrap())).await
                else {
                    return;
                };
                let _ = client
                    .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                    .await;
                let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
            });
        }
    });
    (addr, asked)
}

/// The state, for an assertion message.
fn describe(state: &ConnectionState) -> String {
    match state {
        ConnectionState::Connected(_) => "Connected".to_string(),
        ConnectionState::Failed(reason) => format!("Failed: {reason}"),
        _ => "neither Connected nor Failed".to_string(),
    }
}

fn config(url: &str, root: &[u8]) -> Config {
    let mut config = Config::new(url.parse().unwrap());
    config.root_cert = Some(vec![root.to_vec()]);
    config.connect_timeout = Some(Duration::from_secs(5));
    config
}

#[tokio::test]
async fn a_proxy_mode_client_reaches_the_api_server_through_the_proxy() {
    let (port, root, accepted) = spawn_api_server().await;
    let (proxy, asked) = spawn_proxy().await;

    // Bound: the real URL - a name only the proxy can resolve - through the proxy.
    let mut bound = config(&format!("https://{API_HOST}:{port}"), &root);
    route_through_tunnel(&mut bound, proxy, TunnelRoute::Proxy);
    let state = probe(bound).await;
    assert!(
        matches!(state, ConnectionState::Connected(_)),
        "connected through the proxy, TLS validated against {API_HOST}: {}",
        describe(&state)
    );
    assert_eq!(*asked.lock(), [format!("{API_HOST}:{port}")]);
    assert_eq!(accepted.load(Ordering::SeqCst), 1);

    // Unbound, in the same process: straight to the server, the proxy untouched.
    let mut direct = config(&format!("https://127.0.0.1:{port}"), &root);
    direct.tls_server_name = Some(API_HOST.to_string());
    let state = probe(direct).await;
    assert!(
        matches!(state, ConnectionState::Connected(_)),
        "{}",
        describe(&state)
    );
    assert_eq!(
        asked.lock().len(),
        1,
        "the unbound client never used the proxy"
    );
    assert_eq!(accepted.load(Ordering::SeqCst), 2);
}

/// The `Proxy refuses the connection` scenario: the failure names the proxy, and
/// nothing reaches the API server directly instead.
#[tokio::test]
async fn a_refusing_proxy_fails_the_connection_with_no_fallback() {
    let (port, root, accepted) = spawn_api_server().await;
    let (proxy, asked) = spawn_proxy().await;

    let mut bound = config(&format!("https://elsewhere.fernrohr.test:{port}"), &root);
    route_through_tunnel(&mut bound, proxy, TunnelRoute::Proxy);
    let state = name_the_proxy(probe(bound).await, proxy);
    let ConnectionState::Failed(reason) = state else {
        panic!(
            "a refused CONNECT must fail the connection, got {}",
            describe(&state)
        );
    };
    assert_eq!(asked.lock().len(), 1, "the proxy was asked");
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        0,
        "no direct connection: {reason}"
    );
    assert!(
        reason.starts_with(&format!("the tunnel's proxy at {proxy} refused")),
        "{reason}"
    );
}
