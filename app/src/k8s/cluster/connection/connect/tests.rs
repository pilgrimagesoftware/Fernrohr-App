// Named imports rather than `use super::*`: `gpui_kit::*` next to `#[gpui_kit::test]`
// would shadow the built-in `#[test]` for these plain synchronous/tokio tests.
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::connection::connect::{
    connect_and_probe, resolve_bound_context, resolve_named_context, rewrite_for_tunnel,
};
use crate::k8s::cluster::connection::test_support::{config_for, respond_once, version_info_json};
use kube::Config;
use kube::config::Kubeconfig;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};

fn two_context_kubeconfig() -> Kubeconfig {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let yaml = r#"
apiVersion: v1
kind: Config
clusters:
  - name: kind-dev
    cluster:
      server: https://127.0.0.1:6443
  - name: staging
    cluster:
      server: https://staging.example.com:6443
contexts:
  - name: kind-dev
    context:
      cluster: kind-dev
      user: kind-dev
  - name: staging
    context:
      cluster: staging
      user: staging
current-context: kind-dev
users:
  - name: kind-dev
    user: {}
  - name: staging
    user: {}
"#;
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("fernrohr-connection-fixture-{n}.yaml"));
    std::fs::write(&path, yaml).unwrap();
    Kubeconfig::read_from(&path).unwrap()
}

#[test]
fn resolve_bound_context_prefers_the_passed_context_name() {
    // Deterministic regardless of this machine's real kubeconfig/current-context:
    // `Some(name)` short-circuits before `current_context_name` is ever consulted.
    assert_eq!(
        resolve_bound_context(Some("staging".to_string())),
        Some("staging".to_string())
    );
}

#[tokio::test]
async fn resolve_named_context_picks_that_context_not_current_context() {
    let config = resolve_named_context(two_context_kubeconfig(), "staging")
        .await
        .unwrap();
    assert_eq!(config.cluster_url.host(), Some("staging.example.com"));
}

#[tokio::test]
async fn resolve_named_context_reports_an_unknown_context() {
    let error = resolve_named_context(two_context_kubeconfig(), "does-not-exist")
        .await
        .unwrap_err();
    assert!(error.contains("does-not-exist"));
}

/// Section 6.2: `connect_and_probe`'s wait/success/fail behavior for a tunnel-bound
/// context, driven directly (no GPUI, no real kubeconfig) against a fake watch
/// channel standing in for a `ManagedForward`'s state and a fake HTTP server
/// standing in for the API server - the same seam `ForwardSupervisor`'s own tests
/// (section 2.3) drive, plugged in here one layer up.
mod connect_and_probe_tests {
    use super::*;
    use crate::forward::managed::ForwardState;

    async fn recv_all(mut rx: mpsc::Receiver<ConnectionState>) -> Vec<&'static str> {
        let mut labels = Vec::new();
        while let Some(state) = rx.recv().await {
            labels.push(match state {
                ConnectionState::Connecting => "Connecting",
                ConnectionState::WaitingForTunnel => "WaitingForTunnel",
                ConnectionState::Connected(_) => "Connected",
                ConnectionState::Failed(_) => "Failed",
            });
        }
        labels
    }

    #[tokio::test]
    async fn waits_then_succeeds_once_the_forward_reaches_up() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            respond_once(stream, version_info_json().await, "200 OK").await;
        });

        let (state_tx, state_rx) = watch::channel(ForwardState::Connecting);
        let (tx, rx) = mpsc::channel(4);
        let handle = tokio::spawn(connect_and_probe(
            Ok(config_for(addr)),
            Some((state_rx, addr)),
            tx,
        ));

        // Give connect_and_probe a moment to report the wait before releasing it.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        state_tx.send(ForwardState::Up).unwrap();
        handle.await.unwrap();

        assert_eq!(recv_all(rx).await, vec!["WaitingForTunnel", "Connected"]);
    }

    #[tokio::test]
    async fn stays_waiting_when_the_tunnel_never_comes_up() {
        let (_state_tx, state_rx) = watch::channel(ForwardState::Reconnecting);
        let (tx, mut rx) = mpsc::channel(4);
        // _state_tx is kept alive (never dropped) so state_rx.changed() blocks
        // rather than erroring - the "sustained failure" case from forward_supervisor's
        // own tests, one layer up: the forward keeps retrying, never reaching Up.
        let _task = tokio::spawn(connect_and_probe(
            Ok(config_for("127.0.0.1:1".parse().unwrap())),
            Some((state_rx, "127.0.0.1:1".parse().unwrap())),
            tx,
        ));

        let first = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
            .await
            .expect("should report WaitingForTunnel promptly")
            .unwrap();
        assert!(matches!(first, ConnectionState::WaitingForTunnel));

        let second = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await;
        assert!(
            second.is_err(),
            "must not report Connected/Failed while the forward is still retrying"
        );
    }

    #[tokio::test]
    async fn unbound_context_probes_directly_with_no_wait() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            respond_once(stream, version_info_json().await, "200 OK").await;
        });

        let (tx, rx) = mpsc::channel(4);
        connect_and_probe(Ok(config_for(addr)), None, tx).await;

        assert_eq!(recv_all(rx).await, vec!["Connected"]);
    }

    #[test]
    fn rewrite_pins_tls_server_name_to_the_original_host_and_points_at_local_addr() {
        let mut config = Config::new("https://api.example.com:6443".parse().unwrap());
        let local_addr: std::net::SocketAddr = "127.0.0.1:54321".parse().unwrap();

        rewrite_for_tunnel(&mut config, local_addr);

        assert_eq!(config.cluster_url.to_string(), "https://127.0.0.1:54321/");
        assert_eq!(config.tls_server_name.as_deref(), Some("api.example.com"));
    }

    #[test]
    fn rewrite_does_not_override_an_explicit_tls_server_name() {
        let mut config = Config::new("https://api.example.com:6443".parse().unwrap());
        config.tls_server_name = Some("already-pinned.internal".to_string());
        let local_addr: std::net::SocketAddr = "127.0.0.1:54321".parse().unwrap();

        rewrite_for_tunnel(&mut config, local_addr);

        assert_eq!(
            config.tls_server_name.as_deref(),
            Some("already-pinned.internal")
        );
    }
}
