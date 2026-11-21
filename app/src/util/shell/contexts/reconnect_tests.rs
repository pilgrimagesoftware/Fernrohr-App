//! #416 through a real workspace window: a context whose connection failed is
//! tried again - from its capsule's menu, and by Reconnect Failed Contexts on
//! a key bound in `keymap.toml` - in the same connection, while a context that
//! hasn't failed is left alone.
//!
//! The retry is real, so the failed context's name is one no kubeconfig or
//! `tunnels.toml` has: the attempt can only fail to resolve, with a reason of
//! its own, never reproduce the one set here.

use super::awaiting_tests::{Harness, click, dialog_open, keys};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, init};
use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};

const FAILED: &str = "fernrohr-test-reconnect-416";
const REASON: &str = "connection refused (set by the test)";
const KEY: &str = "ctrl-alt-shift-r";

/// A window holding `FAILED`, failed, and `qa`, still connecting, with
/// Reconnect Failed Contexts bound to [`KEY`].
fn harness(cx: &mut TestAppContext) -> Harness {
    harness_with(cx, ConnectionState::Failed(REASON.into()))
}

/// [`harness`] with `FAILED` in `state` instead.
fn harness_with(cx: &mut TestAppContext, state: ConnectionState) -> Harness {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    std::fs::write(
        &keymap,
        format!("[bindings]\n\"context.reconnect_failed\" = \"{KEY}\"\n"),
    )
    .unwrap();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        ClusterRegistry::insert_test_session(cx, FAILED, state);
        ClusterRegistry::insert_test_session(cx, "qa", ConnectionState::Connecting);
    });
    let names = vec![FAILED.to_string(), "qa".to_string()];
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(names, window, cx));
        main.update(cx, |main, cx| main.focus_initial(window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    let vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    Harness { vcx, main }
}

fn connection(h: &mut Harness, context: &str) -> Entity<ClusterConnection> {
    h.vcx
        .update(|_, cx| ClusterRegistry::existing_connection(cx, context).expect("an open session"))
}

/// Whether `connection` has been tried again: it's connecting, or failed
/// for another reason than the one the test set.
fn retried(h: &mut Harness, connection: &Entity<ClusterConnection>) -> bool {
    h.vcx.update(|_, cx| match &connection.read(cx).state {
        ConnectionState::Failed(reason) => reason != REASON,
        _ => true,
    })
}

#[gpui_kit::test]
async fn a_bound_key_retries_only_the_failed_context(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let (failed, qa) = (connection(&mut h, FAILED), connection(&mut h, "qa"));

    keys(&mut h, KEY);

    assert!(
        retried(&mut h, &failed),
        "the failed context is tried again"
    );
    assert_eq!(connection(&mut h, FAILED), failed, "in the same connection");
    assert_eq!(connection(&mut h, "qa"), qa);
    h.vcx.update(|_, cx| {
        assert!(matches!(qa.read(cx).state, ConnectionState::Connecting));
    });
}

/// The failed capsule's menu leads with Reconnect.
#[gpui_kit::test]
async fn the_failed_capsules_menu_reconnects_it(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let failed = connection(&mut h, FAILED);

    click(&mut h, "context-chip-menu-fernrohr-test-reconnect-416");
    keys(&mut h, "down enter");

    assert!(retried(&mut h, &failed));
    assert!(!dialog_open(&mut h), "Reconnect doesn't ask");
}

/// A capsule that hasn't failed has no Reconnect: its menu leads with
/// Disconnect, which asks first.
#[gpui_kit::test]
async fn a_capsule_that_hasnt_failed_has_no_reconnect(cx: &mut TestAppContext) {
    let mut h = harness(cx);

    click(&mut h, "context-chip-menu-qa");
    keys(&mut h, "down enter");

    assert!(dialog_open(&mut h), "the first row is Disconnect");
}

/// Why the palette would list Reconnect Failed Contexts dimmed, if it would.
fn reconnect_unavailable(h: &mut Harness) -> Option<String> {
    h.vcx.update(|window, cx| {
        crate::command::unavailable::reason(
            &crate::ui::status_bar::ReconnectFailedContexts,
            window,
            cx,
        )
        .map(|reason| reason.to_string())
    })
}

/// With a failed context, the palette offers Reconnect Failed Contexts.
#[gpui_kit::test]
async fn reconnect_is_available_while_a_context_has_failed(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    assert_eq!(reconnect_unavailable(&mut h), None);
}

/// With none failed, it's listed dimmed, saying why.
#[gpui_kit::test]
async fn reconnect_is_unavailable_while_no_context_has_failed(cx: &mut TestAppContext) {
    let mut h = harness_with(cx, ConnectionState::Connecting);
    assert_eq!(
        reconnect_unavailable(&mut h).as_deref(),
        Some("No context in this window has failed.")
    );
}
