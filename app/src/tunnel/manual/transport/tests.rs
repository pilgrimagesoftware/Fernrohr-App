//! The transport on its own: the reachability shortcut against a local listener,
//! Proceed, Cancel, and withdrawing a confirmation nobody waits on any more. The
//! main thread's side - applying the events - is `ManualConfirmations`', tested
//! through the tunnel registry in `k8s::cluster::tunnel`.

use super::*;
use crate::tunnel::manual::confirmations::ConfirmationEvent;
use tokio::net::TcpListener;

const FAST: Duration = Duration::from_millis(300);

fn transport(
    probe: Option<ProbeTarget>,
) -> (ManualTransport, mpsc::UnboundedReceiver<ConfirmationEvent>) {
    let (events, rx) = mpsc::unbounded_channel();
    let transport = ManualTransport::new(
        "corp-vpn-id".into(),
        "corp-vpn".into(),
        Some("Connect the corporate VPN".into()),
        probe,
        events,
    )
    .with_probe_timeout(FAST);
    (transport, rx)
}

/// A port nothing listens on, held for as long as the socket lives: bound but
/// never listening, so a connect to it is refused at once. Binding and letting
/// go (#449) freed the port for a test running alongside to take for its own
/// listener, and the probe then reached that and skipped the prompt.
fn closed_port() -> (tokio::net::TcpSocket, u16) {
    let socket = tokio::net::TcpSocket::new_v4().unwrap();
    socket.bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let port = socket.local_addr().unwrap().port();
    (socket, port)
}

fn local(port: u16) -> ProbeTarget {
    ProbeTarget {
        host: "127.0.0.1".into(),
        port,
    }
}

/// Answers the next published confirmation with `decision`, returning what it asked.
async fn answer(
    rx: &mut mpsc::UnboundedReceiver<ConfirmationEvent>,
    decision: Decision,
) -> PendingRequest {
    loop {
        match rx.recv().await.expect("a confirmation is published") {
            ConfirmationEvent::Pending { request, resolve } => {
                resolve.send(decision).unwrap();
                return request;
            }
            ConfirmationEvent::Settled { .. } | ConfirmationEvent::Withdrawn { .. } => {}
        }
    }
}

#[tokio::test]
async fn a_reachable_api_server_skips_the_prompt() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (mut transport, mut rx) = transport(Some(local(port)));

    assert_eq!(transport.connect_outcome().await, Ok(()));
    match rx.try_recv() {
        Ok(ConfirmationEvent::Settled { tunnel_id }) => assert_eq!(tunnel_id, "corp-vpn-id"),
        _ => panic!("expected the tunnel settled without a prompt"),
    }
    assert!(rx.try_recv().is_err(), "nothing was published");
}

#[tokio::test]
async fn an_unreachable_api_server_asks_and_proceed_connects() {
    let (_held, port) = closed_port();
    let (mut transport, mut rx) = transport(Some(local(port)));
    let asking = tokio::spawn(async move { transport.connect_outcome().await });
    let request = answer(&mut rx, Decision::Proceed).await;
    assert_eq!(request.name, "corp-vpn");
    assert_eq!(
        request.message.as_deref(),
        Some("Connect the corporate VPN")
    );
    assert_eq!(asking.await.unwrap(), Ok(()));
}

#[tokio::test]
async fn with_the_shortcut_off_it_asks_even_when_reachable() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let _port = listener.local_addr().unwrap().port();
    // No probe target: the setting is off.
    let (mut transport, mut rx) = transport(None);
    let asking = tokio::spawn(async move { transport.connect_outcome().await });
    answer(&mut rx, Decision::Proceed).await;
    assert_eq!(asking.await.unwrap(), Ok(()));
}

#[tokio::test]
async fn cancel_gives_up_naming_the_tunnel() {
    let (mut transport, mut rx) = transport(None);
    let asking = tokio::spawn(async move { transport.connect_outcome().await });
    answer(&mut rx, Decision::Cancel).await;
    assert_eq!(
        asking.await.unwrap(),
        Err(ConnectFailure::GiveUp("corp-vpn was cancelled".into()))
    );
}

#[tokio::test]
async fn dropping_a_waiting_transport_withdraws_its_confirmation() {
    let (mut transport, mut rx) = transport(None);
    let asking = tokio::spawn(async move { transport.connect_outcome().await });
    let published = match rx.recv().await.unwrap() {
        ConfirmationEvent::Pending { request, .. } => request.id,
        _ => panic!("expected a confirmation"),
    };
    asking.abort();
    let _ = asking.await;
    match rx.recv().await {
        Some(ConfirmationEvent::Withdrawn { id }) => assert_eq!(id, published),
        _ => panic!("expected the confirmation withdrawn"),
    }
}

/// The closed port stays this test's: no listener can take it while held, and
/// the probe finds it refused rather than reachable (#449).
#[tokio::test]
async fn a_held_closed_port_cannot_be_taken_and_is_unreachable() {
    let (_held, port) = closed_port();
    assert!(
        std::net::TcpListener::bind(("127.0.0.1", port)).is_err(),
        "another listener can't take the held port"
    );
    assert!(!reachable(&local(port), FAST).await, "refused, not reached");
}

#[tokio::test]
async fn the_probe_gives_up_after_its_timeout() {
    // A non-routable address: the connect hangs rather than being refused.
    let target = ProbeTarget {
        host: "10.255.255.1".into(),
        port: 6443,
    };
    let started = std::time::Instant::now();
    assert!(!reachable(&target, FAST).await);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "bounded by its timeout"
    );
}
