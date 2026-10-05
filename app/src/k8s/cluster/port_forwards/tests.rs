//! The row-started forward's lifecycle against a fake cluster serving a Running
//! pod: started, it comes up and listens locally; started again, it's the same
//! forward; stopped, it's released - out of the list and the live set, its
//! listener closed. The data path itself is `forward::k8s::port_forward`'s tests.

use super::{PortForwardRequest, PortForwards};
use crate::forward::managed::ForwardState;
use crate::k8s::test_cluster::FakeCluster;
use gpui_kit::TestAppContext;
use serde_json::json;

fn request() -> PortForwardRequest {
    PortForwardRequest {
        context_name: "demo".into(),
        namespace: "shop".into(),
        pod: "web-1".into(),
        // Not a well-known port, so the local listener can usually take the same number.
        remote_port: 18_080,
    }
}

#[gpui_kit::test]
async fn a_started_forward_comes_up_and_stopping_releases_it(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply(
        "/api/v1",
        "pods",
        json!({ "apiVersion": "v1", "kind": "Pod",
            "metadata": { "name": "web-1", "namespace": "shop", "uid": "u1" },
            "spec": { "containers": [{ "name": "app", "image": "nginx" }] },
            "status": { "phase": "Running" } }),
    );
    let forwards = cx.update(PortForwards::entity);

    let addr = forwards
        .update(cx, |forwards, cx| {
            forwards.start(request(), client.clone(), cx)
        })
        .expect("started");
    let again = forwards
        .update(cx, |forwards, cx| {
            forwards.start(request(), client.clone(), cx)
        })
        .expect("the running one");
    assert_eq!(again, addr, "starting it again returns the same forward");

    for _ in 0..400 {
        cx.run_until_parked();
        let up = forwards.read_with(cx, |forwards, _| {
            forwards.list().first().map(|(_, _, state)| *state) == Some(ForwardState::Up)
        });
        if up {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    forwards.read_with(cx, |forwards, _| {
        let list = forwards.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, request());
        assert_eq!(list[0].2, ForwardState::Up, "the pod is Running");
        assert!(forwards.live_requests().contains(&request()));
    });
    std::net::TcpStream::connect(addr).expect("it listens locally");

    let stopped = forwards.update(cx, |forwards, cx| forwards.stop(&request(), cx));
    assert!(stopped);
    cx.run_until_parked();
    forwards.read_with(cx, |forwards, _| {
        assert!(forwards.list().is_empty(), "gone from the list");
        assert!(forwards.live_requests().is_empty(), "and released");
    });
    let mut closed = false;
    for _ in 0..400 {
        if std::net::TcpStream::connect(addr).is_err() {
            closed = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(closed, "the listener closed");
}
