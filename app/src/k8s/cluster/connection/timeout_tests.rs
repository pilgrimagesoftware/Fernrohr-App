//! #198: a connect timeout says what timed out, after how long, and what to
//! check, keeping the original error; any other failure is unchanged.

use super::{Dialled, explain, is_connect_timeout};
use std::time::Duration;

const RAW: &str =
    "ServiceError: client error (Connect): client error (Connect): deadline has elapsed";

fn dialled(proxy: Option<&str>, through_tunnel: bool) -> Dialled {
    Dialled {
        endpoint: "api.cluster.example:443".into(),
        proxy: proxy.map(str::to_string),
        through_tunnel,
        timeout: Some(Duration::from_secs(30)),
    }
}

#[test]
fn timeouts_are_recognised() {
    assert!(is_connect_timeout(RAW));
    assert!(is_connect_timeout(
        "tcp connect error: Operation timed out (os error 60)"
    ));
    assert!(!is_connect_timeout(
        "tcp connect error: Connection refused (os error 61)"
    ));
}

#[test]
fn a_direct_timeout_names_the_server_and_what_to_check() {
    let message = explain(RAW.into(), &dialled(None, false));
    assert!(
        message.starts_with(
            "Timed out after 30s connecting to the API server at api.cluster.example:443."
        ),
        "{message}"
    );
    assert!(message.contains("VPN"), "{message}");
    assert!(
        message.ends_with(&format!("({RAW})")),
        "the original error is kept"
    );
}

#[test]
fn a_proxied_timeout_names_the_proxy() {
    let message = explain(
        RAW.into(),
        &dialled(Some("http://proxy.example:3128/"), false),
    );
    assert!(
        message.contains("through the proxy http://proxy.example:3128."),
        "{message}"
    );
    assert!(message.contains("NO_PROXY"), "{message}");
}

#[test]
fn a_tunnelled_timeout_names_the_tunnel() {
    let mut through = dialled(None, true);
    through.endpoint = "127.0.0.1:61234".into();
    let message = explain(RAW.into(), &through);
    assert!(
        message.contains("the tunnel's local port 127.0.0.1:61234"),
        "{message}"
    );
    assert!(message.contains("tunnel is up"), "{message}");
}

#[test]
fn other_failures_are_unchanged() {
    let refused = "ServiceError: client error (Connect): Connection refused".to_string();
    assert_eq!(explain(refused.clone(), &dialled(None, false)), refused);
}
