//! `SecretValue` never prints its value, and a reveal keeps exactly one key.

use crate::k8s::resource::secret_value::{RevealError, SecretValue, reveal};
use serde_json::json;

const PASSWORD: &str = "hunter2-hunter2";

#[test]
fn debug_and_display_print_the_size_never_the_value() {
    let value = SecretValue::new(PASSWORD.as_bytes().to_vec());
    let debug = format!("{value:?}");
    let display = format!("{value}");
    assert!(!debug.contains(PASSWORD) && !display.contains(PASSWORD));
    assert_eq!(debug, "<secret: 15 bytes>");
    assert_eq!(display, debug);
    // Nor through a container that derives `Debug`.
    let nested = format!("{:?}", Some(vec![("password", value)]));
    assert!(!nested.contains(PASSWORD), "{nested}");
}

#[test]
fn expose_is_the_way_to_the_text_and_binary_has_none() {
    assert_eq!(
        SecretValue::new(PASSWORD.as_bytes().to_vec()).expose(),
        Some(PASSWORD)
    );
    assert_eq!(SecretValue::new(vec![0xff, 0xfe, 0x00]).expose(), None);
}

/// A fixture API server serving one Secret with two keys, and 404 for any
/// other name.
async fn serve() -> std::net::SocketAddr {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 4096];
                let read = stream.read(&mut buffer).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let line = request.lines().next().unwrap_or_default().to_string();
                let (status, body) = if line.contains("/api/v1/namespaces/staging/secrets/db ") {
                    (
                        "200 OK",
                        json!({
                            "apiVersion": "v1",
                            "kind": "Secret",
                            "metadata": { "name": "db", "namespace": "staging" },
                            // "hunter2-hunter2" and "other-value"
                            "data": {
                                "password": "aHVudGVyMi1odW50ZXIy",
                                "username": "b3RoZXItdmFsdWU=",
                            },
                        }),
                    )
                } else {
                    (
                        "404 Not Found",
                        json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure",
                                "reason": "NotFound", "code": 404, "message": "not found" }),
                    )
                };
                let body = body.to_string();
                let response = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    addr
}

fn run<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

fn client(addr: std::net::SocketAddr) -> kube::Client {
    kube::Client::try_from(kube::Config::new(format!("http://{addr}").parse().unwrap())).unwrap()
}

/// 1.2: a reveal decodes the one key asked for, and holds nothing else - its
/// result is that one value, and it prints as a size.
#[test]
fn a_reveal_keeps_only_the_key_asked_for() {
    run(async {
        let addr = serve().await;
        let value = reveal(
            client(addr),
            "staging".into(),
            "db".into(),
            "password".into(),
        )
        .await
        .expect("the key exists");
        assert_eq!(value.expose(), Some(PASSWORD));
        let printed = format!("{value:?}");
        assert!(!printed.contains("other-value") && !printed.contains(PASSWORD));
    });
}

#[test]
fn a_missing_key_or_secret_is_missing() {
    run(async {
        let addr = serve().await;
        let key = reveal(client(addr), "staging".into(), "db".into(), "nope".into()).await;
        assert!(matches!(key, Err(RevealError::Missing)));
        let secret = reveal(
            client(addr),
            "staging".into(),
            "gone".into(),
            "password".into(),
        )
        .await;
        assert!(matches!(secret, Err(RevealError::Missing)));
    });
}
