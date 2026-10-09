//! A fake API server for tests that check what a request looked like: it
//! records every request's method, path and body, and answers each with one
//! fixed response. Where `test_cluster` models a cluster that changes, this
//! one pins the shape of a write - a delete's options, an apply's patch.

use parking_lot::Mutex;
use std::sync::Arc;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// One request as the server saw it.
#[derive(Clone, Debug)]
pub(crate) struct Recorded {
    pub(crate) method: String,
    /// The path and query, as sent.
    pub(crate) target: String,
    pub(crate) content_type: Option<String>,
    pub(crate) body: String,
}

impl Recorded {
    /// The body parsed as JSON, or `Null` when it isn't JSON.
    pub(crate) fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

/// The server: its requests so far, and the response it gives every one.
#[derive(Clone)]
pub(crate) struct Recorder {
    requests: Arc<Mutex<Vec<Recorded>>>,
}

impl Recorder {
    /// Starts a server answering every request with `status` (`"200 OK"`) and
    /// `body`, on `handle`'s runtime, and returns it with a client for it.
    pub(crate) fn start(
        handle: &tokio::runtime::Handle,
        status: &'static str,
        body: serde_json::Value,
    ) -> (Self, kube::Client) {
        Self::start_answering(handle, move |_| (status, body.clone()))
    }

    /// [`Self::start`], answering each request with what `answer` gives for
    /// it - for a client that parses typed objects, whose `kind` must match.
    pub(crate) fn start_answering(
        handle: &tokio::runtime::Handle,
        answer: impl Fn(&Recorded) -> (&'static str, serde_json::Value) + Send + Sync + 'static,
    ) -> (Self, kube::Client) {
        let recorder = Self {
            requests: Arc::default(),
        };
        let requests = recorder.requests.clone();
        let answer = Arc::new(answer);
        let addr = handle.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                loop {
                    let Ok((mut stream, _)) = listener.accept().await else {
                        return;
                    };
                    let (requests, answer) = (requests.clone(), answer.clone());
                    tokio::spawn(async move {
                        let request = read_request(&mut stream).await;
                        let (status, body) = answer(&request);
                        let body = body.to_string();
                        requests.lock().push(request);
                        let response = format!(
                            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                    });
                }
            });
            addr
        });
        let _guard = handle.enter();
        let client =
            kube::Client::try_from(kube::Config::new(format!("http://{addr}").parse().unwrap()))
                .unwrap();
        (recorder, client)
    }

    pub(crate) fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().clone()
    }
}

/// Reads one request: its head, then as much body as its content-length says.
pub(crate) async fn read_request(stream: &mut tokio::net::TcpStream) -> Recorded {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let read = stream.read(&mut chunk).await.unwrap_or(0);
        if read == 0 {
            break buffer.len();
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..head_end.min(buffer.len())]).to_string();
    let header = |name: &str| {
        head.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name)
                .then(|| value.trim().to_string())
        })
    };
    let length: usize = header("content-length")
        .and_then(|length| length.parse().ok())
        .unwrap_or(0);
    while buffer.len() < head_end + length {
        let read = stream.read(&mut chunk).await.unwrap_or(0);
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let mut line = head.lines().next().unwrap_or_default().split(' ');
    Recorded {
        method: line.next().unwrap_or_default().to_string(),
        target: line.next().unwrap_or_default().to_string(),
        content_type: header("content-type"),
        body: String::from_utf8_lossy(&buffer[head_end.min(buffer.len())..]).to_string(),
    }
}
