//! `manual-confirmation-tunnels` 5.1, end to end: two contexts bound to one manual
//! tunnel connect through the real tunnel registry and connect path against fake
//! API servers answering a recorded `/version` response, while an unrelated
//! context connects during the wait - then Proceed connects both, or Cancel fails
//! both without a request reaching their servers.

use super::{ForwardWait, connect_and_probe};
use crate::config::tunnels::{ManualTunnelConfig, TunnelConfig, TunnelKind};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::connection::test_support::{config_for, respond_once, version_info_json};
use crate::k8s::cluster::tunnel::acquire_for_context;
use crate::tunnel::manual::{Decision, ManualConfirmations};
use crate::tunnel::store::TunnelStore;
use gpui_kit::TestAppContext;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::runtime::Handle;
use tokio::sync::mpsc;

/// A fake API server answering every request with the recorded `/version`
/// response, counting the requests it gets.
struct FakeApi {
    addr: SocketAddr,
    requests: Arc<AtomicUsize>,
}

impl FakeApi {
    fn start(rt: &Handle) -> Self {
        let requests = Arc::new(AtomicUsize::new(0));
        let counted = requests.clone();
        let addr = rt.block_on(async move {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    counted.fetch_add(1, Ordering::SeqCst);
                    tokio::spawn(respond_once(stream, version_info_json().await, "200 OK"));
                }
            });
            addr
        });
        Self { addr, requests }
    }

    fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

/// `corp-vpn`, a manual tunnel that always asks, bound to `dev` and `qa`, whose
/// kubeconfig entries point at their fake servers.
fn bind_corp_vpn(dev: &FakeApi, qa: &FakeApi) -> (std::path::PathBuf, std::path::PathBuf) {
    let tunnels = crate::util::test_paths::temp_path("manual-e2e-tunnels");
    let store = TunnelStore::new(tunnels.clone());
    store
        .create(
            "corp-vpn-id",
            TunnelConfig {
                name: "corp-vpn".into(),
                kind: TunnelKind::Manual,
                manual: ManualTunnelConfig {
                    message: Some("Connect the corporate VPN".into()),
                    skip_when_reachable: false,
                },
                ..TunnelConfig::default()
            },
            None,
        )
        .unwrap();
    store.bind("dev", "corp-vpn-id").unwrap();
    store.bind("qa", "corp-vpn-id").unwrap();
    let kubeconfig =
        crate::util::test_paths::temp_path("manual-e2e-kubeconfig").with_extension("yaml");
    let entry = |name: &str, addr: SocketAddr| {
        (
            format!("  - name: {name}\n    cluster:\n      server: http://{addr}\n"),
            format!("  - name: {name}\n    context:\n      cluster: {name}\n      user: {name}\n"),
            format!("  - name: {name}\n    user: {{}}\n"),
        )
    };
    let (dev, qa) = (entry("dev", dev.addr), entry("qa", qa.addr));
    std::fs::write(
        &kubeconfig,
        format!(
            "apiVersion: v1\nkind: Config\nclusters:\n{}{}contexts:\n{}{}users:\n{}{}",
            dev.0, qa.0, dev.1, qa.1, dev.2, qa.2
        ),
    )
    .unwrap();
    (tunnels, kubeconfig)
}

struct Scene {
    rt: Handle,
    dev: FakeApi,
    qa: FakeApi,
    other: FakeApi,
    dev_states: mpsc::Receiver<ConnectionState>,
    qa_states: mpsc::Receiver<ConnectionState>,
    /// The two connections' forward handles, held as `ClusterConnection` holds them.
    _handles: Vec<
        crate::forward::registry::RegistryHandle<
            crate::k8s::cluster::tunnel::ForwardKey,
            crate::k8s::cluster::tunnel::TunnelForward,
        >,
    >,
}

/// `dev` and `qa` connecting through `corp-vpn`, both waiting on its one prompt.
fn both_waiting(cx: &mut TestAppContext) -> Scene {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::runtime::init(cx);
        ManualConfirmations::init(cx);
    });
    let rt = cx.update(|cx| crate::runtime::handle(cx));
    let (dev, qa, other) = (
        FakeApi::start(&rt),
        FakeApi::start(&rt),
        FakeApi::start(&rt),
    );
    let (tunnels, kubeconfig) = bind_corp_vpn(&dev, &qa);

    let mut handles = Vec::new();
    let mut receivers = Vec::new();
    for (context, api) in [("dev", &dev), ("qa", &qa)] {
        let handle = cx
            .update(|cx| acquire_for_context(cx, &tunnels, Some(&kubeconfig), context))
            .expect("the acquire succeeds")
            .expect("the context is bound");
        let (tx, rx) = mpsc::channel(4);
        rt.spawn(connect_and_probe(
            Ok(config_for(api.addr)),
            Some(ForwardWait::of(&handle)),
            tx,
        ));
        handles.push(handle);
        receivers.push(rx);
    }
    for _ in 0..400 {
        cx.run_until_parked();
        if waiting_contexts(cx) == ["dev", "qa"] {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(waiting_contexts(cx), ["dev", "qa"], "one prompt for both");
    let qa_states = receivers.pop().unwrap();
    let dev_states = receivers.pop().unwrap();
    Scene {
        rt,
        dev,
        qa,
        other,
        dev_states,
        qa_states,
        _handles: handles,
    }
}

fn waiting_contexts(cx: &mut TestAppContext) -> Vec<String> {
    cx.update(|cx| {
        ManualConfirmations::entity(cx)
            .and_then(|entity| {
                let confirmations = entity.read(cx);
                (confirmations.pending().len() == 1)
                    .then(|| confirmations.pending()[0].contexts.clone())
            })
            .unwrap_or_default()
    })
}

/// The next state `rx` reports, within a few seconds.
fn next(rt: &Handle, rx: &mut mpsc::Receiver<ConnectionState>) -> ConnectionState {
    rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("a state arrives")
            .expect("the connection reports")
    })
}

/// While `dev` and `qa` wait, a context bound to no tunnel connects at once - and
/// the waiting ones haven't sent a request.
fn an_unrelated_context_connects_meanwhile(scene: &mut Scene) {
    let (tx, mut rx) = mpsc::channel(4);
    scene.rt.spawn(connect_and_probe(
        Ok(config_for(scene.other.addr)),
        None,
        tx,
    ));
    assert!(matches!(
        next(&scene.rt, &mut rx),
        ConnectionState::Connected(_)
    ));
    assert_eq!(scene.other.requests(), 1);
    for rx in [&mut scene.dev_states, &mut scene.qa_states] {
        assert!(matches!(
            next(&scene.rt, rx),
            ConnectionState::WaitingForTunnel
        ));
        assert!(rx.try_recv().is_err(), "still waiting");
    }
    assert_eq!((scene.dev.requests(), scene.qa.requests()), (0, 0));
}

#[gpui_kit::test]
async fn proceed_connects_both_contexts_sharing_the_tunnel(cx: &mut TestAppContext) {
    let mut scene = both_waiting(cx);
    an_unrelated_context_connects_meanwhile(&mut scene);

    assert!(cx.update(|cx| ManualConfirmations::resolve(cx, "corp-vpn-id", Decision::Proceed)));
    for rx in [&mut scene.dev_states, &mut scene.qa_states] {
        assert!(matches!(next(&scene.rt, rx), ConnectionState::Connected(_)));
    }
    assert_eq!((scene.dev.requests(), scene.qa.requests()), (1, 1));
    cx.run_until_parked();
    assert!(waiting_contexts(cx).is_empty(), "the prompt is answered");
}

#[gpui_kit::test]
async fn cancel_fails_both_contexts_without_a_request(cx: &mut TestAppContext) {
    let mut scene = both_waiting(cx);
    an_unrelated_context_connects_meanwhile(&mut scene);

    assert!(cx.update(|cx| ManualConfirmations::resolve(cx, "corp-vpn-id", Decision::Cancel)));
    for rx in [&mut scene.dev_states, &mut scene.qa_states] {
        match next(&scene.rt, rx) {
            ConnectionState::Failed(reason) => assert_eq!(reason, "corp-vpn was cancelled"),
            _ => panic!("expected the connection cancelled"),
        }
    }
    assert_eq!(
        (scene.dev.requests(), scene.qa.requests()),
        (0, 0),
        "never connected directly instead"
    );
}
