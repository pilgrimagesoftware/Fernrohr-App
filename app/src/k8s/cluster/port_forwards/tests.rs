//! The row-started forward's lifecycle against a fake cluster serving a Running
//! pod: started, it comes up and listens locally; started again, it's the same
//! forward; stopped, it's released - out of the list and the live set, its
//! listener closed. The data path itself is `forward::k8s::port_forward`'s tests.

use super::{ForwardObject, PortForwardRequest, PortForwards};
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

fn origin() -> ForwardObject {
    ForwardObject::pod("demo", "shop", "web-1")
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
            forwards.start(request(), origin(), client.clone(), cx)
        })
        .expect("started");
    let again = forwards
        .update(cx, |forwards, cx| {
            forwards.start(request(), origin(), client.clone(), cx)
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

/// `port-forward-indicators` 1.1: the per-object index follows starts and stops -
/// a pod's forwards by the pod they reach, a Service's by where they were started -
/// and every observer hears each change, as Manage Tunnels' Stop and each row and
/// panel showing the forward all observe the one list.
#[gpui_kit::test]
async fn the_index_follows_starts_and_stops_and_every_observer_hears(cx: &mut TestAppContext) {
    use std::cell::Cell;
    use std::rc::Rc;

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
    let heard = [Rc::new(Cell::new(0)), Rc::new(Cell::new(0))];
    for count in &heard {
        let count = count.clone();
        cx.update(|cx| {
            cx.observe(&forwards, move |_, _| count.set(count.get() + 1))
                .detach()
        });
    }
    let service = ForwardObject::service("demo", "shop", "web");
    let other_pod = ForwardObject::pod("demo", "shop", "web-2");
    let request = PortForwardRequest {
        remote_port: 18_082,
        ..request()
    };

    forwards
        .update(cx, |forwards, cx| {
            forwards.start(request.clone(), origin(), client.clone(), cx)
        })
        .expect("started");
    cx.run_until_parked();
    forwards.read_with(cx, |forwards, _| {
        let on_pod = forwards.for_object(&origin());
        assert_eq!(on_pod.len(), 1);
        assert_eq!(on_pod[0].request, request);
        assert_eq!(on_pod[0].target_port, 18_082);
        assert!(
            forwards.for_object(&service).is_empty(),
            "not started from the Service"
        );
        assert!(forwards.for_object(&other_pod).is_empty());
    });
    assert!(
        heard.iter().all(|count| count.get() > 0),
        "both heard the start"
    );

    // The same forward, started again from the Service in front of the pod.
    forwards
        .update(cx, |forwards, cx| {
            forwards.start(request.clone(), service.clone(), client.clone(), cx)
        })
        .expect("the running one");
    forwards.read_with(cx, |forwards, _| {
        assert_eq!(
            forwards.for_object(&service).len(),
            1,
            "now on the Service too"
        );
        assert_eq!(forwards.for_object(&origin()).len(), 1, "still one forward");
        assert_eq!(forwards.via_service(&request).as_deref(), Some("web"));
    });

    let before: Vec<usize> = heard.iter().map(|count| count.get()).collect();
    forwards.update(cx, |forwards, cx| forwards.stop(&request, cx));
    cx.run_until_parked();
    forwards.read_with(cx, |forwards, _| {
        assert!(forwards.for_object(&origin()).is_empty());
        assert!(forwards.for_object(&service).is_empty());
    });
    for (count, before) in heard.iter().zip(before) {
        assert!(count.get() > before, "every observer heard the stop");
    }
}
