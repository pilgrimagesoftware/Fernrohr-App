//! A stand-in Kubernetes API server for tests: answers fixed responses by request
//! path, each with its own HTTP status, and lets a test change a route while it
//! runs - an aggregated API group that answers 503, then recovers. Plain threads
//! and `std::net`, so a tokio test and a gpui test can both start one.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};

type Routes = Arc<Mutex<HashMap<String, (u16, String)>>>;

/// A running server. It serves until the process ends; tests are short.
#[derive(Clone)]
pub(crate) struct MockApi {
    pub(crate) addr: SocketAddr,
    routes: Routes,
}

impl MockApi {
    /// Starts serving `routes` (path, status, body) on a free local port.
    pub(crate) fn start(routes: &[(&str, u16, &str)]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a free local port");
        let addr = listener.local_addr().expect("a bound address");
        let routes: Routes = Arc::new(Mutex::new(
            routes
                .iter()
                .map(|(path, status, body)| (path.to_string(), (*status, body.to_string())))
                .collect(),
        ));
        let served = routes.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let routes = served.clone();
                std::thread::spawn(move || answer(stream, &routes));
            }
        });
        Self { addr, routes }
    }

    /// Changes what `path` answers from now on.
    pub(crate) fn set(&self, path: &str, status: u16, body: &str) {
        self.routes
            .lock()
            .expect("routes lock")
            .insert(path.to_string(), (status, body.to_string()));
    }

    /// A client for this server, with a short connect timeout and kube's retries
    /// off, so a 503 comes back at once rather than after its backoff.
    pub(crate) fn client(&self) -> kube::Client {
        self.client_with_retries(false)
    }

    /// [`Self::client`], with kube's default retrying of 429/503/504 on or off.
    pub(crate) fn client_with_retries(&self, retries: bool) -> kube::Client {
        let mut config = kube::Config::new(format!("http://{}", self.addr).parse().unwrap());
        config.connect_timeout = Some(std::time::Duration::from_millis(500));
        config.default_retry = retries;
        kube::Client::try_from(config).expect("a client for the mock server")
    }
}

fn answer(stream: std::net::TcpStream, routes: &Routes) {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
        return;
    }
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
    }
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .to_string();
    let (status, body) = routes
        .lock()
        .expect("routes lock")
        .get(&path)
        .cloned()
        .unwrap_or((404, "{}".to_string()));
    let response = format!(
        "HTTP/1.1 {status} Status\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut stream = reader.into_inner();
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.shutdown(std::net::Shutdown::Write);
}

const CORE_VERSIONS: &str =
    r#"{"kind":"APIVersions","versions":["v1"],"serverAddressByClientCIDRs":[]}"#;
const CORE_V1: &str = r#"{"kind":"APIResourceList","groupVersion":"v1","resources":[
    {"name":"pods","singularName":"pod","namespaced":true,"kind":"Pod","verbs":["get","list","watch"]}
]}"#;
const GROUPS: &str = r#"{"kind":"APIGroupList","apiVersion":"v1","groups":[
    {"name":"apps","versions":[{"groupVersion":"apps/v1","version":"v1"}],
     "preferredVersion":{"groupVersion":"apps/v1","version":"v1"}},
    {"name":"metrics.k8s.io","versions":[{"groupVersion":"metrics.k8s.io/v1beta1","version":"v1beta1"}],
     "preferredVersion":{"groupVersion":"metrics.k8s.io/v1beta1","version":"v1beta1"}}
]}"#;
const APPS_V1: &str = r#"{"kind":"APIResourceList","apiVersion":"v1","groupVersion":"apps/v1","resources":[
    {"name":"deployments","singularName":"deployment","namespaced":true,"kind":"Deployment","verbs":["get","list","watch"]}
]}"#;
const METRICS_V1BETA1: &str = r#"{"kind":"APIResourceList","apiVersion":"v1","groupVersion":"metrics.k8s.io/v1beta1","resources":[
    {"name":"pods","singularName":"","namespaced":true,"kind":"PodMetrics","verbs":["get","list"]}
]}"#;

/// A cluster whose aggregated `metrics.k8s.io` backend is down: that group answers
/// 503 with a body that isn't a `Status`, the case that used to fail discovery
/// whole.
pub(crate) fn cluster_with_a_failing_aggregated_group() -> MockApi {
    MockApi::start(&[
        ("/api", 200, CORE_VERSIONS),
        ("/api/v1", 200, CORE_V1),
        ("/apis", 200, GROUPS),
        ("/apis/apps/v1", 200, APPS_V1),
        ("/apis/metrics.k8s.io/v1beta1", 503, "service unavailable"),
    ])
}

/// The aggregated group recovering: its resource list starts answering.
pub(crate) fn recover_metrics(api: &MockApi) {
    api.set("/apis/metrics.k8s.io/v1beta1", 200, METRICS_V1BETA1);
}

/// A cluster that can't list its API groups at all - the one fatal failure.
pub(crate) fn cluster_whose_group_list_fails() -> MockApi {
    MockApi::start(&[
        ("/api", 200, CORE_VERSIONS),
        ("/api/v1", 200, CORE_V1),
        ("/apis", 503, "service unavailable"),
    ])
}
