//! `connect_context` (`mcp-connections` design decision 4 and Security 1, 6)
//! against real workspace windows (`navigate::test_support`) and a scripted
//! user (`approval::scripted`), every call from a session of its own. Each
//! session the test seeds is held by nobody until a window or agent takes
//! it, so `ClusterRegistry::hold` reuses it and no real kubeconfig or cluster
//! is ever reached.
//!
//! Named imports, not `use super::*`: see `util/shell.rs` on the
//! macro-expansion budget.

use crate::config::tunnels::{TunnelConfig, TunnelKind};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::{AgentId, ClusterRegistry, Holder, LiveAgents};
use crate::mcp::approval::scripted::{Script, Scripted};
use crate::mcp::error::{ToolError, Unavailability};
use crate::mcp::navigate::test_support::{Harness, harness_with, workspace};
use crate::mcp::read::test_support::{call, open_unheld};
use crate::mcp::session::Caller;
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

/// `h`'s tools as a fresh agent's session, with `user` answering every
/// question.
fn answered_by(
    cx: &mut TestAppContext,
    h: &Harness,
    script: Script,
) -> (ToolContext, Arc<Scripted>) {
    as_agent(cx, h, script, Duration::from_secs(30))
}

/// A live agent's session (`LiveAgents`, as the endpoint opens one).
fn as_agent(
    cx: &mut TestAppContext,
    h: &Harness,
    script: Script,
    timeout: Duration,
) -> (ToolContext, Arc<Scripted>) {
    let user = Scripted::new(script);
    let agent = AgentId::next();
    cx.update(|cx| LiveAgents::open(cx, agent, Some("assistant".into())));
    let tools = ToolContext {
        approvals: user.gate_with_timeout(timeout),
        caller: Some(Caller {
            agent,
            client_name: Some("assistant".into()),
        }),
        ..h.tools.clone()
    };
    (tools, user)
}

fn agent_of(tools: &ToolContext) -> AgentId {
    tools.caller.as_ref().expect("a session").agent
}

/// Seeds `staging`'s session in `state`, held by nobody yet.
fn staging(cx: &mut TestAppContext, state: ConnectionState) {
    open_unheld(cx, "staging", state, None);
}

async fn connect(cx: &mut TestAppContext, tools: &ToolContext, context: &str) -> ToolResult {
    let result = call(cx, tools, "connect_context", json!({"context": context})).await;
    cx.run_until_parked();
    result
}

fn content(result: ToolResult) -> Map<String, Value> {
    result.expect("the tool succeeded").content
}

fn windows_holding(cx: &mut TestAppContext, context: &str) -> usize {
    cx.update(|cx| ClusterRegistry::window_holder_count(cx, context))
}

fn holders(cx: &mut TestAppContext, context: &str) -> Vec<Holder> {
    cx.update(|cx| ClusterRegistry::holders(cx, context))
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
    let (tools, user) = answered_by(cx, &h, Script::Allow);

    let result = connect(cx, &tools, "nope").await;

    assert_eq!(
        result,
        Err(ToolError::UnknownContext {
            context: "nope".into()
        })
    );
    assert!(user.asked().is_empty());
    assert!(holders(cx, "nope").is_empty());
}

/// A call with no session is refused before anything is asked.
#[gpui_kit::test]
async fn a_call_without_a_session_is_refused(cx: &mut TestAppContext) {
    let h = known(cx);
    let (tools, user) = answered_by(cx, &h, Script::Allow);
    let tools = ToolContext {
        caller: None,
        ..tools
    };
    assert_eq!(
        connect(cx, &tools, "staging").await,
        Err(ToolError::Unavailable {
            reason: Unavailability::NoSession
        })
    );
    assert!(user.asked().is_empty());
}

/// `agent-mcp` "Context already in the window": no question, its window comes
/// forward, and the agent's session holds it too.
#[gpui_kit::test]
async fn a_context_the_window_holds_is_brought_forward_without_asking(cx: &mut TestAppContext) {
    let h = known(cx);
    let (dev, _) = workspace(cx, &["dev"]);
    terminal(cx);
    let (tools, user) = answered_by(cx, &h, Script::Deny);

    let output = content(connect(cx, &tools, "dev").await);

    assert_eq!(output["already_open"], json!(true));
    assert_eq!(output["state"], json!("connected"));
    assert!(
        user.asked().is_empty(),
        "no question for a window's context"
    );
    assert_eq!(windows_holding(cx, "dev"), 1, "no second connection");
    assert!(holders(cx, "dev").contains(&Holder::Agent(agent_of(&tools))));
    assert_eq!(active(cx), Some(dev.window_id()), "its window came forward");
}

/// `agent-mcp` "Connect a context that is not open": the user is asked, and
/// on approval the agent's session alone holds it - no window gains it.
#[gpui_kit::test]
async fn an_approved_connect_is_held_by_the_session_and_no_window(cx: &mut TestAppContext) {
    let h = known(cx);
    let _ = workspace(cx, &["dev"]);
    staging(cx, ConnectionState::Connected(h.client.clone()));
    let (tools, user) = answered_by(cx, &h, Script::Allow);

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
    assert_eq!(
        holders(cx, "staging"),
        vec![Holder::Agent(agent_of(&tools))]
    );
    assert_eq!(
        windows_holding(cx, "staging"),
        0,
        "no window gained it: adding a context to a window holds it"
    );
}

/// A context the session already holds is reported without a question.
#[gpui_kit::test]
async fn a_context_the_session_holds_asks_nothing_again(cx: &mut TestAppContext) {
    let h = known(cx);
    staging(cx, ConnectionState::Connected(h.client.clone()));
    let (tools, user) = answered_by(cx, &h, Script::Allow);
    content(connect(cx, &tools, "staging").await);
    assert_eq!(user.asked().len(), 1);

    let output = content(connect(cx, &tools, "staging").await);

    assert_eq!(output["already_open"], json!(true));
    assert_eq!(user.asked().len(), 1, "not asked a second time");
}

/// `agent-mcp` "Context only another agent holds" and "Denied while another
/// agent holds it" (Security 1): another session's approval never counts.
#[gpui_kit::test]
async fn another_agents_connection_needs_this_sessions_own_approval(cx: &mut TestAppContext) {
    let h = known(cx);
    staging(cx, ConnectionState::Connected(h.client.clone()));
    let (first, _) = answered_by(cx, &h, Script::Allow);
    content(connect(cx, &first, "staging").await);

    let (second, user) = as_agent(cx, &h, Script::Deny, Duration::from_millis(50));
    assert_eq!(
        connect(cx, &second, "staging").await,
        Err(ToolError::Denied)
    );
    assert_eq!(user.asked().len(), 1, "the second agent was asked about");
    assert_eq!(
        holders(cx, "staging"),
        vec![Holder::Agent(agent_of(&first))],
        "denied: the second holds nothing, the first is unchanged"
    );

    let (third, user) = answered_by(cx, &h, Script::Allow);
    let output = content(connect(cx, &third, "staging").await);
    assert_eq!(user.asked().len(), 1);
    assert_eq!(
        output["state"],
        json!("connected"),
        "the connection is reused"
    );
    assert!(holders(cx, "staging").contains(&Holder::Agent(agent_of(&third))));
}

/// `agent-mcp` "Another agent's connection" (Security 6): until it's
/// approved for this session, another agent's context reads as not open.
#[gpui_kit::test]
async fn another_agents_connection_is_not_readable(cx: &mut TestAppContext) {
    let h = known(cx);
    staging(cx, ConnectionState::Connected(h.client.clone()));
    let (first, _) = answered_by(cx, &h, Script::Allow);
    content(connect(cx, &first, "staging").await);
    let (second, _) = answered_by(cx, &h, Script::Deny);

    let listed = content(call(cx, &second, "list_contexts", json!({})).await);
    let staging_status = listed["contexts"]
        .as_array()
        .expect("a list")
        .iter()
        .find(|context| context["name"] == "staging")
        .map(|context| context["status"].clone());
    assert_eq!(staging_status, Some(json!("not_open")));
    assert_eq!(
        call(
            cx,
            &second,
            "list_resource_kinds",
            json!({"context": "staging"})
        )
        .await,
        Err(ToolError::Disconnected {
            context: "staging".into()
        })
    );
    let listed = content(call(cx, &first, "list_contexts", json!({})).await);
    assert!(
        listed["contexts"]
            .as_array()
            .expect("a list")
            .iter()
            .any(|context| context["name"] == "staging" && context["status"] == "connected"),
        "the agent that connected it sees it"
    );
}

#[gpui_kit::test]
async fn a_denied_or_unanswered_connect_starts_nothing(cx: &mut TestAppContext) {
    for (script, error) in [
        (Script::Deny, ToolError::Denied),
        (Script::Never, ToolError::ApprovalTimedOut),
    ] {
        let h = known(cx);
        let _ = workspace(cx, &["dev"]);
        let (tools, _) = as_agent(cx, &h, script, Duration::from_millis(50));

        assert_eq!(connect(cx, &tools, "staging").await, Err(error));
        assert!(holders(cx, "staging").is_empty(), "nothing holds it");
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
    staging(
        cx,
        ConnectionState::Failed("exec plugin refused: token=s3cr3t".into()),
    );
    let (tools, _) = answered_by(cx, &h, Script::Allow);

    let output = content(connect(cx, &tools, "staging").await);

    assert_eq!(output["state"], json!("failed"));
    let reason = output["reason"].as_str().expect("a reason");
    assert!(!reason.contains("s3cr3t"), "{reason}");
    assert!(reason.contains("exec plugin refused"), "{reason}");
}

#[gpui_kit::test]
async fn a_manual_tunnel_awaiting_the_user_is_reported_at_once(cx: &mut TestAppContext) {
    let h = known(cx);
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
    let (tools, user) = answered_by(cx, &h, Script::Allow);

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
    staging(cx, ConnectionState::Connecting);
    let (tools, _) = answered_by(cx, &h, Script::Allow);
    let tools = ToolContext {
        connect_settle: Duration::from_millis(50),
        ..tools
    };

    let output = content(connect(cx, &tools, "staging").await);

    assert_eq!(output["state"], json!("connecting"));
    assert_eq!(
        holders(cx, "staging"),
        vec![Holder::Agent(agent_of(&tools))],
        "the connection carries on, held by the session"
    );
}

/// Review of App#434 F1: the session ends while the user is deciding. Their
/// approval then grants nothing - no hold, no connection - and the call gets
/// the restart error.
#[gpui_kit::test]
async fn an_approval_for_an_ended_session_holds_nothing(cx: &mut TestAppContext) {
    let h = known(cx);
    let (tools, user) = answered_by(cx, &h, Script::Allow);
    // Ended before the approved hold: the client left while the dialog was up.
    cx.update(|cx| LiveAgents::end(cx, agent_of(&tools), Duration::from_secs(30)));

    let result = connect(cx, &tools, "staging").await;

    assert_eq!(user.asked().len(), 1, "the user was asked");
    assert_eq!(
        result,
        Err(ToolError::Unavailable {
            reason: Unavailability::NoSession
        })
    );
    assert!(holders(cx, "staging").is_empty(), "nothing holds it");
    assert!(
        cx.update(|cx| ClusterRegistry::existing_connection(cx, "staging"))
            .is_none(),
        "nothing connected"
    );
}

/// The no-approval path too: an ended session can't join a window's context.
#[gpui_kit::test]
async fn an_ended_session_cant_join_a_windows_context(cx: &mut TestAppContext) {
    let h = known(cx);
    let _ = workspace(cx, &["dev"]);
    let (tools, user) = answered_by(cx, &h, Script::Allow);
    cx.update(|cx| LiveAgents::end(cx, agent_of(&tools), Duration::from_secs(30)));

    let result = connect(cx, &tools, "dev").await;

    assert!(user.asked().is_empty());
    assert_eq!(
        result,
        Err(ToolError::Unavailable {
            reason: Unavailability::NoSession
        })
    );
    assert!(!holders(cx, "dev").contains(&Holder::Agent(agent_of(&tools))));
}

/// `mcp-connections` design decision 4: opening a panel on a context only the
/// session holds adds it to the frontmost window, which then holds it as well.
#[gpui_kit::test]
async fn opening_a_panel_on_the_sessions_context_brings_it_into_a_window(cx: &mut TestAppContext) {
    let h = known(cx);
    let _ = workspace(cx, &["dev"]);
    open_unheld(
        cx,
        "staging",
        ConnectionState::Connected(h.client.clone()),
        Some(crate::mcp::navigate::test_support::core_kinds()),
    );
    let (tools, _) = answered_by(cx, &h, Script::Allow);
    content(connect(cx, &tools, "staging").await);
    assert_eq!(
        windows_holding(cx, "staging"),
        0,
        "the agent's alone so far"
    );

    let opened = call(
        cx,
        &tools,
        "open_panel",
        json!({"context": "staging", "kind": "Pod"}),
    )
    .await;
    cx.run_until_parked();

    assert!(opened.is_ok(), "{opened:?}");
    assert_eq!(windows_holding(cx, "staging"), 1, "a window holds it now");
    assert!(holders(cx, "staging").contains(&Holder::Agent(agent_of(&tools))));
}

/// The same session's tools, with `script` answering instead.
fn answering(
    tools: &ToolContext,
    script: Script,
    timeout: Duration,
) -> (ToolContext, Arc<Scripted>) {
    let user = Scripted::new(script);
    let tools = ToolContext {
        approvals: user.gate_with_timeout(timeout),
        ..tools.clone()
    };
    (tools, user)
}

/// `staging` connected by a fresh agent, held by it alone, with discovery,
/// beside a window on `dev`.
async fn agent_only_staging(cx: &mut TestAppContext) -> (Harness, ToolContext) {
    let h = known(cx);
    let _ = workspace(cx, &["dev"]);
    open_unheld(
        cx,
        "staging",
        ConnectionState::Connected(h.client.clone()),
        Some(crate::mcp::navigate::test_support::core_kinds()),
    );
    let (tools, _) = answered_by(cx, &h, Script::Allow);
    content(connect(cx, &tools, "staging").await);
    (h, tools)
}

async fn open_pods(cx: &mut TestAppContext, tools: &ToolContext, context: &str) -> ToolResult {
    let result = call(
        cx,
        tools,
        "open_panel",
        json!({"context": context, "kind": "Pod"}),
    )
    .await;
    cx.run_until_parked();
    result
}

/// The user's decision on review of App#435: before a context only this agent
/// holds goes into a window (where every agent can then use it), the user is
/// asked, and the question says so.
#[gpui_kit::test]
async fn showing_an_agent_only_context_asks_first(cx: &mut TestAppContext) {
    let (_h, tools) = agent_only_staging(cx).await;
    let (tools, user) = answering(&tools, Script::Allow, Duration::from_secs(30));

    assert!(open_pods(cx, &tools, "staging").await.is_ok());

    let asked = user.asked();
    assert_eq!(asked.len(), 1, "asked once, before showing it");
    assert_eq!(asked[0].context, "staging");
    assert_eq!(asked[0].tool, "open_panel");
    assert!(asked[0].title.contains("Other agents can then use it"));
    assert!(!asked[0].irreversible(), "the recoverable tier");
    assert_eq!(windows_holding(cx, "staging"), 1);
}

/// Denied or unanswered: no window gains it, and the tool says why.
#[gpui_kit::test]
async fn a_refused_show_changes_nothing(cx: &mut TestAppContext) {
    for (script, error) in [
        (Script::Deny, ToolError::Denied),
        (Script::Never, ToolError::ApprovalTimedOut),
    ] {
        let (_h, tools) = agent_only_staging(cx).await;
        let (tools, _) = answering(&tools, script, Duration::from_millis(50));

        assert_eq!(open_pods(cx, &tools, "staging").await, Err(error));
        assert_eq!(windows_holding(cx, "staging"), 0, "no window gained it");
        assert_eq!(
            holders(cx, "staging"),
            vec![Holder::Agent(agent_of(&tools))]
        );
    }
}

/// A context a user window already holds is shown with no question.
#[gpui_kit::test]
async fn showing_a_windows_context_asks_nothing(cx: &mut TestAppContext) {
    let h = known(cx);
    let _ = workspace(cx, &["dev"]);
    let (tools, user) = answered_by(cx, &h, Script::Deny);

    assert!(open_pods(cx, &tools, "dev").await.is_ok());
    assert!(user.asked().is_empty());
}

/// The consequence the user agreed to: once agent A's allowed show puts the
/// context in a window, agent B connects it without a dialog.
#[gpui_kit::test]
async fn after_an_allowed_show_another_agent_connects_without_asking(cx: &mut TestAppContext) {
    let (h, a) = agent_only_staging(cx).await;
    let (a, _) = answering(&a, Script::Allow, Duration::from_secs(30));
    assert!(open_pods(cx, &a, "staging").await.is_ok());

    let (b, user) = answered_by(cx, &h, Script::Deny);
    let output = content(connect(cx, &b, "staging").await);

    assert!(user.asked().is_empty(), "no dialog for B");
    assert_eq!(output["already_open"], json!(true));
    assert!(holders(cx, "staging").contains(&Holder::Agent(agent_of(&b))));
}
