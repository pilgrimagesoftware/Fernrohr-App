//! 7.2-7.3 together: a run writes its kubeconfig for the connection's
//! endpoint, runs `helm` against it and removes it; and actions are refused
//! without a usable `helm`, a connection, or a free release.

use super::{HelmUnavailable, RunSpec, availability, execute, start, start_on};
use crate::helm_cli::kubeconfig::Endpoint;
use crate::helm_cli::locate::{HelmBinary, HelmStatus, set_status};
use crate::helm_cli::run::{HelmFailure, ReleaseKey, RunSlots};
use crate::helm_cli::test_helm::{script, temp_dir};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use gpui_kit::TestAppContext;
use kube::config::Kubeconfig;
use std::net::SocketAddr;
use std::time::Duration;

fn source() -> Kubeconfig {
    Kubeconfig::from_yaml(
        r#"
clusters:
- name: demo-cluster
  cluster: { server: "https://api.example.test:6443" }
users:
- name: demo-user
  user: { token: placeholder-token }
contexts:
- name: demo
  context: { cluster: demo-cluster, user: demo-user }
"#,
    )
    .unwrap()
}

/// `helm` reads the run's kubeconfig - through the tunnel - and the file is
/// gone when the run returns.
#[tokio::test]
async fn a_run_points_helm_through_the_tunnel_and_cleans_up() {
    let dir = temp_dir("start-execute");
    let kubeconfigs = dir.join("kubeconfigs");
    let seen = dir.join("seen.yaml");
    let helm = script(
        &dir,
        "helm",
        &format!("cat \"$KUBECONFIG\" > '{}'", seen.display()),
    );
    let local = SocketAddr::from(([127, 0, 0, 1], 41000));
    execute(
        RunSpec {
            binary: helm,
            args: vec!["uninstall".into(), "web".into(), "-n".into(), "shop".into()],
            context: "demo".into(),
            endpoint: Endpoint::Rewrite(local),
            dir: kubeconfigs.clone(),
            path_env: "/usr/bin:/bin".into(),
            timeout: Duration::from_secs(30),
        },
        &source(),
    )
    .await
    .expect("it runs");
    let read = Kubeconfig::from_yaml(&std::fs::read_to_string(&seen).unwrap()).unwrap();
    let cluster = read.clusters[0].cluster.as_ref().unwrap();
    assert_eq!(cluster.server.as_deref(), Some("https://127.0.0.1:41000"));
    assert_eq!(cluster.tls_server_name.as_deref(), Some("api.example.test"));
    assert_eq!(
        std::fs::read_dir(&kubeconfigs).unwrap().count(),
        0,
        "the run's kubeconfig is removed"
    );
}

fn ready() -> HelmStatus {
    HelmStatus::Ready(HelmBinary {
        path: "/opt/helm/bin/helm".into(),
        version: "v3.14.2".into(),
    })
}

fn release() -> ReleaseKey {
    ReleaseKey {
        context: "demo".into(),
        namespace: "shop".into(),
        release: "web".into(),
    }
}

/// Unavailable while `helm` is being looked for or isn't usable, and while
/// the context isn't connected - each with its reason.
#[gpui_kit::test]
fn actions_are_unavailable_with_a_reason(cx: &mut TestAppContext) {
    cx.update(|cx| {
        assert_eq!(availability("demo", cx), Err(HelmUnavailable::Checking));
        set_status(HelmStatus::Unusable("no helm here".into()), cx);
        assert_eq!(
            availability("demo", cx),
            Err(HelmUnavailable::NoHelm("no helm here".into()))
        );
        set_status(ready(), cx);
        assert_eq!(availability("demo", cx), Err(HelmUnavailable::NotConnected));
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
        assert_eq!(
            availability("demo", cx),
            Err(HelmUnavailable::NotConnected),
            "connecting isn't connected"
        );
    });
}

/// `start` refuses at once, with the reason, when an action can't run.
#[gpui_kit::test]
async fn starting_without_a_connection_is_refused(cx: &mut TestAppContext) {
    let refused = cx.update(|cx| {
        set_status(ready(), cx);
        start(release(), vec!["uninstall".into()], cx)
    });
    assert_eq!(
        refused.await,
        Err(HelmFailure::Start("The context isn't connected.".into()))
    );
}

/// A second run on a release already running is refused, without starting.
#[gpui_kit::test]
async fn a_second_run_on_a_running_release_is_refused(cx: &mut TestAppContext) {
    let slots = RunSlots::default();
    let _running = slots.claim(release()).unwrap();
    let refused = cx.update(|cx| {
        let binary = match ready() {
            HelmStatus::Ready(binary) => binary,
            _ => unreachable!(),
        };
        start_on(&slots, release(), vec![], binary, Endpoint::Direct, cx)
    });
    assert_eq!(refused.await, Err(HelmFailure::AlreadyRunning));
}
