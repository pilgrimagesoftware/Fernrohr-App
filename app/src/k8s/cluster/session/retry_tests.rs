//! #198 and `mcp-connections` design decision 6: connecting a context again
//! after its connection failed (the picker's Connect after "Couldn't connect")
//! redials that same connection in place, whoever holds it, so a window's
//! panels or an agent's session keep the entity they observe. A plain reader of
//! the connection never re-dials.

use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::session::{AgentId, ClusterRegistry, Holder};
use gpui_kit::{Context, TestAppContext, WindowId};
use std::cell::Cell;
use std::rc::Rc;

/// A redial that counts its calls and moves the connection to `Connecting`.
fn stub(
    calls: Rc<Cell<usize>>,
) -> impl FnOnce(&mut ClusterConnection, &mut Context<ClusterConnection>) -> bool {
    move |connection, cx| {
        calls.set(calls.get() + 1);
        connection.state = ConnectionState::Connecting;
        cx.notify();
        true
    }
}

fn failed(cx: &mut gpui_kit::App) -> gpui_kit::Entity<ClusterConnection> {
    ClusterRegistry::insert_test_session(
        cx,
        "demo",
        ConnectionState::Failed("deadline has elapsed".into()),
    )
}

#[gpui_kit::test]
fn a_failed_unheld_session_is_redialled_in_place(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let failed = failed(cx);
        let calls = Rc::new(Cell::new(0));
        let retried = ClusterRegistry::retry_with(cx, "demo", stub(calls.clone()));
        assert_eq!(calls.get(), 1, "dialled again");
        assert_eq!(retried, failed, "the same connection, not a new one");
        assert!(matches!(
            retried.read(cx).state,
            ConnectionState::Connecting
        ));
    });
}

#[gpui_kit::test]
fn a_failed_session_a_window_holds_is_redialled_in_place(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let failed = failed(cx);
        ClusterRegistry::hold(cx, "demo", WindowId::from(1));
        let calls = Rc::new(Cell::new(0));
        let retried = ClusterRegistry::retry_with(cx, "demo", stub(calls.clone()));
        assert_eq!(calls.get(), 1);
        assert_eq!(retried, failed, "its panels observe this connection");
    });
}

/// `cluster-connection` "Retry keeps an agent's hold": a failed connection
/// only an agent holds is redialled in place, so the agent isn't left on an
/// orphaned entity, and its hold is kept.
#[gpui_kit::test]
fn retry_keeps_an_agents_hold(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let failed = failed(cx);
        let agent = AgentId::next();
        ClusterRegistry::hold(cx, "demo", agent);
        let calls = Rc::new(Cell::new(0));
        let retried = ClusterRegistry::retry_with(cx, "demo", stub(calls.clone()));
        assert_eq!(calls.get(), 1);
        assert_eq!(retried, failed, "the entity the agent's session reads");
        assert_eq!(
            ClusterRegistry::holders(cx, "demo"),
            vec![Holder::Agent(agent)],
            "the agent still holds it"
        );
        assert_eq!(
            ClusterRegistry::existing_connection(cx, "demo"),
            Some(failed)
        );
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
        let failed = failed(cx);
        assert_eq!(ClusterRegistry::connection(cx, "demo"), failed);
    });
}
