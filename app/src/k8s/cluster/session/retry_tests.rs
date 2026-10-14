//! #198: connecting a context again after its connection failed - the picker's
//! Connect after "Couldn't connect" - dials afresh instead of handing back the
//! failed connection, unless a window holds that session; a plain reader of
//! the connection never re-dials.

use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::session::ClusterRegistry;
use gpui_kit::{AppContext as _, Entity, TestAppContext, WindowId};
use std::cell::Cell;
use std::rc::Rc;

/// A connect that counts its calls and hands back a fresh `Connecting` one.
fn stub(calls: Rc<Cell<usize>>) -> impl FnOnce(&mut gpui_kit::App) -> Entity<ClusterConnection> {
    move |cx| {
        calls.set(calls.get() + 1);
        cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting))
    }
}

fn is_connecting(connection: &Entity<ClusterConnection>, cx: &gpui_kit::App) -> bool {
    matches!(connection.read(cx).state, ConnectionState::Connecting)
}

#[gpui_kit::test]
fn a_failed_unheld_session_connects_again(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let failed = ClusterRegistry::insert_test_session(
            cx,
            "demo",
            ConnectionState::Failed("deadline has elapsed".into()),
        );
        let calls = Rc::new(Cell::new(0));
        ClusterRegistry::retry_with(cx, "demo", stub(calls.clone()));
        assert_eq!(calls.get(), 1, "dialled again");
        let now = ClusterRegistry::connection(cx, "demo");
        assert_ne!(now, failed, "not the failed connection");
        assert!(is_connecting(&now, cx));
    });
}

#[gpui_kit::test]
fn a_failed_session_a_window_holds_is_kept(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let failed = ClusterRegistry::insert_test_session(
            cx,
            "demo",
            ConnectionState::Failed("deadline has elapsed".into()),
        );
        // Held as `ClusterRegistry::hold` leaves it, without its own connect.
        cx.global_mut::<ClusterRegistry>()
            .sessions
            .get_mut("demo")
            .expect("inserted above")
            .holders
            .insert(WindowId::from(1));
        let calls = Rc::new(Cell::new(0));
        ClusterRegistry::retry_with(cx, "demo", stub(calls.clone()));
        assert_eq!(calls.get(), 0, "its panels observe that connection");
        assert_eq!(ClusterRegistry::connection(cx, "demo"), failed);
    });
}

#[gpui_kit::test]
fn a_live_session_is_reused(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let connecting =
            ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
        let calls = Rc::new(Cell::new(0));
        ClusterRegistry::retry_with(cx, "demo", stub(calls.clone()));
        assert_eq!(calls.get(), 0);
        assert_eq!(ClusterRegistry::connection(cx, "demo"), connecting);
    });
}

#[gpui_kit::test]
fn reading_a_failed_connection_never_redials(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let failed = ClusterRegistry::insert_test_session(
            cx,
            "demo",
            ConnectionState::Failed("deadline has elapsed".into()),
        );
        assert_eq!(ClusterRegistry::connection(cx, "demo"), failed);
    });
}
