//! `connect_context` against real workspace windows (`navigate::test_support`)
//! and a scripted user (`approval::scripted`), with every session seeded by
//! the test before it connects, so `ClusterRegistry::hold` reuses it and no
//! real kubeconfig or cluster is ever reached.
//!
//! Named imports, not `use super::*`: see `util/shell.rs` on the
//! macro-expansion budget.

use crate::config::tunnels::{TunnelConfig, TunnelKind};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::mcp::approval::scripted::{Script, Scripted};
use crate::mcp::error::ToolError;
use crate::mcp::navigate::test_support::{Harness, harness_with, workspace};
use crate::mcp::read::test_support::{call, open};
use crate::mcp::tools::{ToolContext, ToolResult};
use crate::tunnel::manual::ManualConfirmations;
use crate::tunnel::store::TunnelStore;
use gpui_kit::{AnyWindowHandle, Context, IntoElement, Render, TestAppContext, Window, div};
use serde_json::{Map, Value, json};
use std::sync::Arc;
use std::time::Duration;

/// A kubeconfig with `dev` and `staging`, `dev` connected.
fn known(cx: &mut TestAppContext) -> Harness {
    harness_with(cx, &["dev", "staging"], &["dev"])
}

/// `h`'s tools, with `user` answering every question.
fn answered_by(h: &Harness, script: Script) -> (ToolContext, Arc<Scripted>) {
    let user = Scripted::new(script);
    let tools = ToolContext {
        approvals: user.gate_with_timeout(Duration::from_secs(30)),
        ..h.tools.clone()
    };
    (tools, user)
}

/// Seeds `staging`'s session in `state`, held by no window yet.
fn staging(cx: &mut TestAppContext, state: ConnectionState) {
    open(cx, "staging", state, None);
}

async fn connect(cx: &mut TestAppContext, tools: &ToolContext, context: &str) -> ToolResult {
    let result = call(cx, tools, "connect_context", json!({"context": context})).await;
    cx.run_until_parked();
    result
}

fn content(result: ToolResult) -> Map<String, Value> {
    result.expect("the tool succeeded").content
}

fn holders(cx: &mut TestAppContext, context: &str) -> usize {
    cx.update(|cx| ClusterRegistry::holder_count(cx, context))
}

struct Terminal;

impl Render for Terminal {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// A non-workspace window standing in for the agent's terminal, made active.
fn terminal(cx: &mut TestAppContext) {
    let window: AnyWindowHandle = cx.add_window(|_, _| Terminal).into();
    cx.update(|cx| {
        window
            .update(cx, |_, window, _| window.activate_window())
            .unwrap()
    });
    cx.run_until_parked();
}

fn active(cx: &mut TestAppContext) -> Option<gpui_kit::WindowId> {
    cx.update(|cx| cx.active_window())
        .map(|window| window.window_id())
}

#[gpui_kit::test]
async fn an_unknown_context_asks_nothing_and_connects_nothing(cx: &mut TestAppContext) {
    let h = known(cx);
    let _ = workspace(cx, &["dev"]);
    let (tools, user) = answered_by(&h, Script::Allow);

    let result = connect(cx, &tools, "nope").await;

    assert_eq!(
        result,
        Err(ToolError::UnknownContext {
            context: "nope".into()
        })
    );
    assert!(user.asked().is_empty());
    assert_eq!(holders(cx, "nope"), 0);
}

#[gpui_kit::test]
async fn a_context_the_window_holds_is_brought_forward_without_asking(cx: &mut TestAppContext) {
    let h = known(cx);
    let (dev, _) = workspace(cx, &["dev"]);
    terminal(cx);
    let (tools, user) = answered_by(&h, Script::Deny);

    let output = content(connect(cx, &tools, "dev").await);

    assert_eq!(output["already_open"], json!(true));
    assert_eq!(output["state"], json!("connected"));
    assert!(user.asked().is_empty(), "no question for a held context");
    assert_eq!(holders(cx, "dev"), 1, "no second connection");
    assert_eq!(active(cx), Some(dev.window_id()), "its window came forward");
}

#[gpui_kit::test]
async fn an_approved_connect_joins_the_frontmost_window(cx: &mut TestAppContext) {
    let h = known(cx);
    let (dev, _) = workspace(cx, &["dev"]);
    let client = h.client.clone();
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "staging", ConnectionState::Connected(client))
    });
    terminal(cx);
    let (tools, user) = answered_by(&h, Script::Allow);

    let output = content(connect(cx, &tools, "staging").await);

    assert_eq!(output["state"], json!("connected"));
    assert_eq!(output["already_open"], json!(false));
    let asked = user.asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].context, "staging");
    assert!(!asked[0].irreversible(), "the recoverable tier");
    assert_eq!(
        asked[0].parameters,
        [("Tunnel".to_string(), "None (direct connection)".to_string())]
    );
    assert_eq!(holders(cx, "staging"), 1, "the window holds it now");
    assert_eq!(active(cx), Some(dev.window_id()));
}

#[gpui_kit::test]
async fn a_denied_or_unanswered_connect_starts_nothing(cx: &mut TestAppContext) {
    for (script, error) in [
        (Script::Deny, ToolError::Denied),
        (Script::Never, ToolError::ApprovalTimedOut),
    ] {
        let h = known(cx);
        let _ = workspace(cx, &["dev"]);
        let user = Scripted::new(script);
        let tools = ToolContext {
            approvals: user.gate_with_timeout(Duration::from_millis(50)),
            ..h.tools.clone()
        };

        assert_eq!(connect(cx, &tools, "staging").await, Err(error));
        assert_eq!(holders(cx, "staging"), 0, "nothing connected");
        assert!(
            cx.update(|cx| ClusterRegistry::existing_connection(cx, "staging"))
                .is_none(),
            "no session was started"
        );
    }
}

#[gpui_kit::test]
async fn a_failed_connect_reports_a_redacted_reason(cx: &mut TestAppContext) {
    let h = known(cx);
    let _ = workspace(cx, &["dev"]);
    staging(
        cx,
        ConnectionState::Failed("exec plugin refused: token=s3cr3t".into()),
    );
    let (tools, _) = answered_by(&h, Script::Allow);

    let output = content(connect(cx, &tools, "staging").await);

    assert_eq!(output["state"], json!("failed"));
    let reason = output["reason"].as_str().expect("a reason");
    assert!(!reason.contains("s3cr3t"), "{reason}");
    assert!(reason.contains("exec plugin refused"), "{reason}");
}

#[gpui_kit::test]
async fn a_manual_tunnel_awaiting_the_user_is_reported_at_once(cx: &mut TestAppContext) {
    let h = known(cx);
    let _ = workspace(cx, &["dev"]);
    let tunnels = h.tools.tunnels.clone().expect("a test tunnels.toml");
    let store = TunnelStore::new(tunnels);
    store
        .create(
            "corp-vpn-id",
            TunnelConfig {
                name: "corp-vpn".into(),
                kind: TunnelKind::Manual,
                ..Default::default()
            },
            None,
        )
        .expect("the manual tunnel saves");
    store.bind("staging", "corp-vpn-id").expect("binds");
    staging(cx, ConnectionState::WaitingForTunnel);
    let _answer = cx.update(|cx| {
        ManualConfirmations::insert_test_pending(cx, "corp-vpn-id", "corp-vpn", None, &["staging"])
    });
    let (tools, user) = answered_by(&h, Script::Allow);

    let output = content(connect(cx, &tools, "staging").await);

    assert_eq!(output["state"], json!("awaiting_confirmation"));
    assert_eq!(
        user.asked()[0].parameters,
        [("Tunnel".to_string(), "corp-vpn (manual tunnel)".to_string())],
        "the question named the tunnel and its kind"
    );
}

#[gpui_kit::test]
async fn a_connection_still_going_when_the_wait_ends_is_connecting(cx: &mut TestAppContext) {
    let h = known(cx);
    let _ = workspace(cx, &["dev"]);
    staging(cx, ConnectionState::Connecting);
    let (tools, _) = answered_by(&h, Script::Allow);
    let tools = ToolContext {
        connect_settle: Duration::from_millis(50),
        ..tools
    };

    let output = content(connect(cx, &tools, "staging").await);

    assert_eq!(output["state"], json!("connecting"));
    assert_eq!(holders(cx, "staging"), 1, "the connection carries on");
}

#[gpui_kit::test]
async fn with_no_window_open_a_window_opens_on_the_context(cx: &mut TestAppContext) {
    let h = known(cx);
    let client = h.client.clone();
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "staging", ConnectionState::Connected(client))
    });
    let (tools, _) = answered_by(&h, Script::Allow);

    let output = content(connect(cx, &tools, "staging").await);

    assert_eq!(output["state"], json!("connected"));
    assert_eq!(holders(cx, "staging"), 1, "a new window holds it");
}
