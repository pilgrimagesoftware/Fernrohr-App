//! Test doubles for #188: a fake HTTP server that records every request (a
//! fake OIDC issuer, or a fake API server to see the `Authorization` it gets),
//! fake JWTs, and a kubeconfig fixture on disk. Every token here is made up and
//! every URL is loopback.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// One request the server got.
#[derive(Clone, Debug)]
pub(in crate::k8s::cluster) struct Recorded {
    pub(in crate::k8s::cluster) method: String,
    pub(in crate::k8s::cluster) path: String,
    /// Header names lower-cased.
    pub(in crate::k8s::cluster) headers: HashMap<String, String>,
    pub(in crate::k8s::cluster) body: String,
}

impl Recorded {
    /// The form body's `key`, percent-decoded.
    pub(in crate::k8s::cluster) fn form(&self, key: &str) -> Option<String> {
        self.body.split('&').find_map(|pair| {
            let (name, value) = pair.split_once('=')?;
            (name == key).then(|| {
                percent_encoding::percent_decode_str(value)
                    .decode_utf8_lossy()
                    .into_owned()
            })
        })
    }
}

type Handler = dyn Fn(&Recorded, SocketAddr) -> (u16, String) + Send + Sync;

/// A server answering each request with `handler`'s (status, JSON body).
#[derive(Clone)]
pub(in crate::k8s::cluster) struct FakeServer {
    pub(in crate::k8s::cluster) addr: SocketAddr,
    requests: Arc<Mutex<Vec<Recorded>>>,
}

impl FakeServer {
    pub(in crate::k8s::cluster) fn start(
        handler: impl Fn(&Recorded, SocketAddr) -> (u16, String) + Send + Sync + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a free local port");
        let addr = listener.local_addr().expect("a bound address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let handler: Arc<Handler> = Arc::new(handler);
        let recorded = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (handler, recorded) = (handler.clone(), recorded.clone());
                std::thread::spawn(move || answer(stream, addr, &*handler, &recorded));
            }
        });
        Self { addr, requests }
    }

    pub(in crate::k8s::cluster) fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub(in crate::k8s::cluster) fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().clone()
    }
}

fn answer(stream: TcpStream, addr: SocketAddr, handler: &Handler, recorded: &Mutex<Vec<Recorded>>) {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.trim_end().split_once(':') {
            headers.insert(name.trim().to_lowercase(), value.trim().to_string());
        }
    }
    let length = headers
        .get("content-length")
        .and_then(|length| length.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    let _ = reader.read_exact(&mut body);
    let request = Recorded {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    };
    let (status, body) = handler(&request, addr);
    recorded.lock().push(request);
    let response = format!(
        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = reader.get_mut().write_all(response.as_bytes());
}

/// A JWT-shaped fake id-token expiring at `exp` (seconds since the epoch);
/// `tag` tells tokens apart. Unsigned - nothing here checks signatures.
pub(in crate::k8s::cluster) fn jwt(exp: i64, tag: &str) -> String {
    let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none"}"#);
    let payload = URL_SAFE_NO_PAD.encode(format!(r#"{{"exp":{exp},"sub":"{tag}"}}"#));
    format!("{header}.{payload}.fake-signature")
}

/// Seconds since the epoch, `offset` seconds from now.
pub(in crate::k8s::cluster) fn from_now(offset: i64) -> i64 {
    jiff::Timestamp::now().as_second() + offset
}

/// A fake issuer at `/dex`: discovery names its `/dex/token` endpoint, which
/// trades `refresh_token` for `new_id_token` and the rotated
/// `new_refresh_token` - for client `fernrohr` with secret `fake-secret`,
/// given either in a Basic header or in the form - and refuses anything else
/// with `invalid_grant`. Like Dex, it rotates: `refresh_token` works once.
pub(in crate::k8s::cluster) fn fake_issuer(
    refresh_token: &'static str,
    new_id_token: String,
    new_refresh_token: &'static str,
) -> FakeServer {
    FakeIssuer {
        refresh_token,
        new_id_token,
        new_refresh_token,
        claimed_issuer: None,
        token_delay: std::time::Duration::ZERO,
    }
    .start()
}

/// [`fake_issuer`], configurable.
pub(in crate::k8s::cluster) struct FakeIssuer {
    pub(in crate::k8s::cluster) refresh_token: &'static str,
    pub(in crate::k8s::cluster) new_id_token: String,
    pub(in crate::k8s::cluster) new_refresh_token: &'static str,
    /// The `issuer` its discovery document names; its own URL when `None`.
    pub(in crate::k8s::cluster) claimed_issuer: Option<String>,
    /// How long the token endpoint takes - so two renewals overlap.
    pub(in crate::k8s::cluster) token_delay: std::time::Duration,
}

impl FakeIssuer {
    pub(in crate::k8s::cluster) fn start(self) -> FakeServer {
        use base64::engine::general_purpose::STANDARD;
        use std::sync::atomic::{AtomicBool, Ordering};
        let basic = format!("Basic {}", STANDARD.encode("fernrohr:fake-secret"));
        let spent = AtomicBool::new(false);
        FakeServer::start(move |request, addr| {
            match (request.method.as_str(), request.path.as_str()) {
                ("GET", "/dex/.well-known/openid-configuration") => {
                    let issuer = self
                        .claimed_issuer
                        .clone()
                        .unwrap_or_else(|| format!("http://{addr}/dex"));
                    (
                        200,
                        format!(
                            r#"{{"issuer":"{issuer}","token_endpoint":"http://{addr}/dex/token"}}"#
                        ),
                    )
                }
                ("POST", "/dex/token") => {
                    std::thread::sleep(self.token_delay);
                    let client_ok = request.headers.get("authorization") == Some(&basic)
                        || (request.form("client_id").as_deref() == Some("fernrohr")
                            && request.form("client_secret").as_deref() == Some("fake-secret"));
                    let grant_ok = request.form("grant_type").as_deref() == Some("refresh_token")
                        && request.form("refresh_token").as_deref() == Some(self.refresh_token);
                    if client_ok && grant_ok && !spent.swap(true, Ordering::SeqCst) {
                        (
                            200,
                            format!(
                                r#"{{"id_token":"{}","refresh_token":"{}","token_type":"bearer"}}"#,
                                self.new_id_token, self.new_refresh_token
                            ),
                        )
                    } else {
                        (400, r#"{"error":"invalid_grant"}"#.to_string())
                    }
                }
                _ => (404, "{}".to_string()),
            }
        })
    }
}

/// A kubeconfig with one context, `dex`, whose user `dex-user` logs in through
/// the oidc auth-provider at `issuer` with `id_token` and `refresh_token`, its
/// cluster at `server`.
pub(in crate::k8s::cluster) fn oidc_kubeconfig(
    server: &str,
    issuer: &str,
    id_token: &str,
    refresh_token: &str,
) -> String {
    format!(
        r#"apiVersion: v1
kind: Config
clusters:
- name: dex-cluster
  cluster:
    server: {server}
contexts:
- name: dex
  context:
    cluster: dex-cluster
    user: dex-user
current-context: dex
users:
- name: other-user
  user:
    token: not-oidc
- name: dex-user
  user:
    auth-provider:
      name: oidc
      config:
        client-id: fernrohr
        client-secret: fake-secret
        id-token: {id_token}
        idp-issuer-url: {issuer}
        refresh-token: {refresh_token}
"#
    )
}

/// A fresh directory for one test's files, removed when dropped.
pub(in crate::k8s::cluster) struct TempDir(PathBuf);

impl TempDir {
    pub(in crate::k8s::cluster) fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "fernrohr-oidc-{name}-{}-{}",
            std::process::id(),
            jiff::Timestamp::now().as_nanosecond()
        ));
        std::fs::create_dir_all(&dir).expect("a temp dir");
        Self(dir)
    }

    /// Writes `text` to `name` in the directory, owner-only as a kubeconfig is.
    pub(in crate::k8s::cluster) fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, text).expect("a fixture file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .expect("fixture permissions");
        }
        path
    }

    pub(in crate::k8s::cluster) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
