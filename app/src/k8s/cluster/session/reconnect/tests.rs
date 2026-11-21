//! #416: retrying a failed context a window holds re-dials inside the
//! connection its panels observe; nothing happens without a session, and a
//! connection that hasn't failed isn't re-dialled.

use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::session::ClusterRegistry;
use gpui_kit::{AppContext as _, TestAppContext, WindowId};
use std::cell::Cell;
use std::rc::Rc;

#[gpui_kit::test]
fn a_held_failed_context_is_retried_in_the_same_connection(cx: &mut TestAppContext) {
    let notified = Rc::new(Cell::new(0));
    let (failed, _observation) = cx.update(|cx| {
        let failed = ClusterRegistry::insert_test_session(
            cx,
            "demo",
            ConnectionState::Failed("connection refused".into()),
        );
        cx.global_mut::<ClusterRegistry>()
            .sessions
            .get_mut("demo")
            .expect("inserted above")
            .holders
            .insert(WindowId::from(1).into());
        let observation = cx.observe(&failed, {
            let notified = notified.clone();
            move |_, _| notified.set(notified.get() + 1)
        });
        (failed, observation)
    });

    let retried = cx.update(|cx| {
        ClusterRegistry::reconnect_with(cx, "demo", |connection, cx| {
            connection.state = ConnectionState::Connecting;
            cx.notify();
            true
        })
    });

    assert!(retried);
    // Observers hear of it once the update's effects flush.
    assert!(notified.get() > 0, "observers saw the retry");
    cx.update(|cx| {
        let now = ClusterRegistry::connection(cx, "demo");
        assert_eq!(now, failed, "the connection its panels observe");
        assert!(matches!(now.read(cx).state, ConnectionState::Connecting));
        assert_eq!(
            ClusterRegistry::window_holder_count(cx, "demo"),
            1,
            "still held"
        );
    });
}

#[gpui_kit::test]
fn a_context_with_no_session_is_left_alone(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let called = Rc::new(Cell::new(false));
        let retried = ClusterRegistry::reconnect_with(cx, "absent", {
            let called = called.clone();
            move |_, _| {
                called.set(true);
                true
            }
        });
        assert!(!retried);
        assert!(!called.get(), "nothing to redial");
        assert!(ClusterRegistry::existing_connection(cx, "absent").is_none());
    });
}

/// The real redial: only a failed connection goes back to connecting - a
/// second retry while the first runs does nothing.
#[gpui_kit::test]
fn a_connection_that_hasnt_failed_isnt_redialled(cx: &mut TestAppContext) {
    cx.update(|cx| {
        for state in [
            ConnectionState::Connecting,
            ConnectionState::WaitingForTunnel,
        ] {
            let connection = cx.new(|_| ClusterConnection::test_with_state(state));
            assert!(!connection.update(cx, |connection, cx| connection.redial(cx)));
            assert!(!matches!(
                connection.read(cx).state,
                ConnectionState::Failed(_)
            ));
        }
        assert!(!ClusterRegistry::reconnect(cx, "absent"));
    });
}
