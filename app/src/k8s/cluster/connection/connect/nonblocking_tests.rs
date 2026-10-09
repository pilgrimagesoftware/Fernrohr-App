//! `manual-confirmation-tunnels` 2.4: a connection waiting on a manual tunnel holds
//! nothing else up. While one context waits for its confirmation, an unbound context
//! connects and loads, and the app keeps running its main thread.

use super::{ForwardWait, connect_and_probe};
use crate::config::tunnels::{ManualTunnelConfig, TunnelConfig, TunnelKind};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::connection::test_support::{config_for, respond_once, version_info_json};
use crate::k8s::cluster::tunnel::acquire_for_context;
use crate::tunnel::manual::{Decision, ManualConfirmations};
use crate::tunnel::store::TunnelStore;
use gpui_kit::TestAppContext;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

/// A manual tunnel bound to `vpn-ctx`, whose server nothing answers on.
fn manual_fixture() -> (std::path::PathBuf, std::path::PathBuf) {
    let tunnels = crate::util::test_paths::temp_path("nonblocking-tunnels");
    let store = TunnelStore::new(tunnels.clone());
    store
        .create(
            "corp-vpn-id",
            TunnelConfig {
                name: "corp-vpn".into(),
                kind: TunnelKind::Manual,
                manual: ManualTunnelConfig {
                    message: None,
                    skip_when_reachable: false,
                },
                ..TunnelConfig::default()
            },
            None,
        )
        .unwrap();
    store.bind("vpn-ctx", "corp-vpn-id").unwrap();
    let kubeconfig =
        crate::util::test_paths::temp_path("nonblocking-kubeconfig").with_extension("yaml");
    std::fs::write(
        &kubeconfig,
        "apiVersion: v1\nkind: Config\n\
         clusters:\n  - name: vpn-ctx\n    cluster:\n      server: https://127.0.0.1:1\n\
         contexts:\n  - name: vpn-ctx\n    context:\n      cluster: vpn-ctx\n      user: vpn-ctx\n\
         users:\n  - name: vpn-ctx\n    user: {}\n",
    )
    .unwrap();
    (tunnels, kubeconfig)
}

fn pending_count(cx: &mut TestAppContext) -> usize {
    cx.update(|cx| {
        ManualConfirmations::entity(cx).map_or(0, |entity| entity.read(cx).pending().len())
    })
}

#[gpui_kit::test]
async fn an_unbound_context_connects_while_another_awaits_confirmation(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let rt = cx.update(|cx| crate::runtime::handle(cx));
    let (tunnels, kubeconfig) = manual_fixture();

    // The waiting context: its forward acquired, its connection started.
    let handle = cx
        .update(|cx| acquire_for_context(cx, &tunnels, Some(&kubeconfig), "vpn-ctx"))
        .unwrap()
        .unwrap();
    let (waiting_tx, mut waiting_rx) = mpsc::channel(4);
    let wait = ForwardWait::of(&handle);
    rt.spawn(connect_and_probe(
        Ok(config_for("127.0.0.1:1".parse().unwrap())),
        Some(wait),
        waiting_tx,
    ));
    for _ in 0..400 {
        cx.run_until_parked();
        if pending_count(cx) == 1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        pending_count(cx),
        1,
        "the first context is waiting for Proceed"
    );

    // The unbound context connects and loads while the first still waits.
    let connected = rt
        .block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                respond_once(stream, version_info_json().await, "200 OK").await;
            });
            let (tx, mut rx) = mpsc::channel(4);
            tokio::spawn(connect_and_probe(Ok(config_for(addr)), None, tx));
            tokio::time::timeout(Duration::from_secs(5), rx.recv()).await
        })
        .expect("the unbound context isn't held up");
    assert!(
        matches!(connected, Some(ConnectionState::Connected(_))),
        "the unbound context connected"
    );

    // The main thread still runs, and the first context is still only waiting.
    cx.run_until_parked();
    assert_eq!(pending_count(cx), 1);
    assert!(matches!(
        waiting_rx.try_recv(),
        Ok(ConnectionState::WaitingForTunnel)
    ));
    assert!(
        waiting_rx.try_recv().is_err(),
        "still waiting, nothing more"
    );

    cx.update(|cx| ManualConfirmations::resolve(cx, "corp-vpn-id", Decision::Cancel));
    cx.run_until_parked();
    drop(handle);
}
