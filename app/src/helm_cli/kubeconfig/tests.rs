//! 7.2: the run's kubeconfig points at the connection's effective endpoint
//! for each kind of connection, keeps the user stanza as it was, is written
//! 0600, and is gone once the run ends - or at the next startup after a crash.

use super::{Endpoint, RunKubeconfig, for_run, sweep};
use crate::helm_cli::test_helm::temp_dir;
use crate::k8s::cluster::tunnel::TunnelRoute;
use kube::config::Kubeconfig;
use std::net::SocketAddr;
use std::time::{Duration, SystemTime};

/// A kubeconfig with the `demo` context and an unrelated `other` one, whose
/// user signs in through an exec plugin.
fn source() -> Kubeconfig {
    Kubeconfig::from_yaml(
        r#"
apiVersion: v1
kind: Config
current-context: other
clusters:
- name: demo-cluster
  cluster:
    server: https://api.example.test:6443
    certificate-authority-data: Q0EgREFUQQ==
- name: other-cluster
  cluster:
    server: https://other.example.test
users:
- name: demo-user
  user:
    exec:
      apiVersion: client.authentication.k8s.io/v1beta1
      command: example-login
      args: ["token", "--cluster", "demo"]
- name: other-user
  user:
    token: other-token
contexts:
- name: demo
  context:
    cluster: demo-cluster
    user: demo-user
    namespace: shop
- name: other
  context:
    cluster: other-cluster
    user: other-user
"#,
    )
    .expect("a valid kubeconfig")
}

fn local(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

fn cluster(config: &Kubeconfig) -> &kube::config::Cluster {
    config.clusters[0].cluster.as_ref().expect("a cluster")
}

#[test]
fn each_route_maps_to_its_endpoint() {
    assert_eq!(Endpoint::of(None), Endpoint::Direct);
    assert_eq!(
        Endpoint::of(Some((TunnelRoute::Direct, local(1)))),
        Endpoint::Direct
    );
    assert_eq!(
        Endpoint::of(Some((TunnelRoute::Rewrite, local(2)))),
        Endpoint::Rewrite(local(2))
    );
    assert_eq!(
        Endpoint::of(Some((TunnelRoute::Proxy, local(3)))),
        Endpoint::Proxy(local(3))
    );
}

/// A direct connection: the context's cluster as it is, and nothing else.
#[test]
fn a_direct_connection_keeps_the_cluster() {
    let run = for_run(&source(), "demo", Endpoint::Direct).unwrap();
    assert_eq!(run.current_context.as_deref(), Some("demo"));
    assert_eq!(run.contexts.len(), 1, "only the run's context");
    assert_eq!(run.clusters.len(), 1);
    let cluster = cluster(&run);
    assert_eq!(
        cluster.server.as_deref(),
        Some("https://api.example.test:6443")
    );
    assert_eq!(cluster.tls_server_name, None);
    assert_eq!(cluster.proxy_url, None);
    assert_eq!(
        cluster.certificate_authority_data.as_deref(),
        Some("Q0EgREFUQQ==")
    );
}

/// An SSH tunnel or a forward-mode command tunnel: the local port is the
/// server, verified as the real host.
#[test]
fn a_rewriting_tunnel_points_at_its_port_and_keeps_the_host_name() {
    let run = for_run(&source(), "demo", Endpoint::Rewrite(local(41000))).unwrap();
    let cluster = cluster(&run);
    assert_eq!(cluster.server.as_deref(), Some("https://127.0.0.1:41000"));
    assert_eq!(cluster.tls_server_name.as_deref(), Some("api.example.test"));
    assert_eq!(cluster.proxy_url, None);
    assert_eq!(
        cluster.certificate_authority_data.as_deref(),
        Some("Q0EgREFUQQ=="),
        "the original CA"
    );
}

/// A proxy-mode command tunnel: the real server, through the tunnel's proxy.
#[test]
fn a_proxy_tunnel_sets_the_proxy_url() {
    let run = for_run(&source(), "demo", Endpoint::Proxy(local(41001))).unwrap();
    let cluster = cluster(&run);
    assert_eq!(
        cluster.server.as_deref(),
        Some("https://api.example.test:6443")
    );
    assert_eq!(cluster.proxy_url.as_deref(), Some("http://127.0.0.1:41001"));
    assert_eq!(cluster.tls_server_name, None);
}

/// The user stanza goes across unchanged, exec plugin and all, so `helm`
/// signs in as the context does; no other user goes with it.
#[test]
fn the_user_is_copied_unchanged() {
    let source = source();
    let run = for_run(&source, "demo", Endpoint::Rewrite(local(41000))).unwrap();
    assert_eq!(run.auth_infos.len(), 1);
    let as_json = |value: &kube::config::NamedAuthInfo| serde_json::to_value(value).unwrap();
    assert_eq!(as_json(&run.auth_infos[0]), as_json(&source.auth_infos[0]));
    assert_eq!(
        run.contexts[0]
            .context
            .as_ref()
            .unwrap()
            .namespace
            .as_deref(),
        Some("shop")
    );
}

#[test]
fn an_unknown_context_is_an_error() {
    let refused = for_run(&source(), "nope", Endpoint::Direct).unwrap_err();
    assert!(refused.contains("no context named nope"), "{refused}");
}

/// The file is 0600 from creation, reads back as the same kubeconfig, and is
/// gone once its run is.
#[test]
fn the_file_is_private_and_removed_after_the_run() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = temp_dir("kubeconfig-file");
    let config = for_run(&source(), "demo", Endpoint::Proxy(local(41001))).unwrap();
    let file = RunKubeconfig::write(&dir, &config).expect("written");
    let path = file.path().to_path_buf();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let read = Kubeconfig::read_from(&path).expect("helm can read it");
    assert_eq!(
        read.clusters[0]
            .cluster
            .as_ref()
            .unwrap()
            .proxy_url
            .as_deref(),
        Some("http://127.0.0.1:41001")
    );
    drop(file);
    assert!(!path.exists(), "removed when the run ends");
}

/// Startup removes files from a process that's gone, and any older than a
/// run can last; a live, recent run's file stays.
#[test]
fn the_sweep_keeps_only_files_a_run_could_still_be_using() {
    let dir = temp_dir("kubeconfig-sweep");
    let write = |name: &str| {
        let path = dir.join(name);
        std::fs::write(&path, "kind: Config").unwrap();
        path
    };
    let live = write("100-0.yaml");
    let dead = write("200-0.yaml");
    let stray = write("not-a-run.yaml");
    let alive = |pid: u32| pid == 100;

    let removed = sweep(&dir, SystemTime::now(), alive);
    assert_eq!(removed, 2);
    assert!(live.exists(), "a live, recent run's file stays");
    assert!(!dead.exists() && !stray.exists());

    let later = SystemTime::now() + crate::consts::HELM_ACTION_TIMEOUT + Duration::from_secs(120);
    assert_eq!(sweep(&dir, later, alive), 1, "older than any run can last");
    assert!(!live.exists());
}
