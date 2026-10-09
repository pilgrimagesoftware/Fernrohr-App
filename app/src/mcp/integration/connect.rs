//! `connect_context` through the adapter (`mcp-connect-and-focus` 3.1): an
//! approved connect reaches connected against the fixture API server and
//! joins the frontmost window; a denied one starts nothing.

use super::{Agent, content, error_code};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::mcp::approval::scripted::{Script, Scripted};
use crate::mcp::navigate::test_support::{harness_with, workspace};
use crate::mcp::tools::ToolContext;
use gpui_kit::TestAppContext;
use serde_json::json;
use std::time::Duration;

#[gpui_kit::test]
async fn an_approved_connect_reaches_connected(cx: &mut TestAppContext) {
    let h = harness_with(cx, &["dev", "staging"], &["dev"]);
    let _ = workspace(cx, &["dev"]);
    // `staging`'s session talks to the same recording API server as `dev`'s.
    let client = h.client.clone();
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "staging", ConnectionState::Connected(client))
    });
    let user = Scripted::new(Script::Allow);
    let tools = ToolContext {
        approvals: user.gate_with_timeout(Duration::from_secs(30)),
        ..h.tools.clone()
    };
    let mut agent = Agent::connect(cx, tools).await;

    let result = agent
        .call(cx, "connect_context", json!({"context": "staging"}))
        .await;

    assert_eq!(content(&result)["state"], "connected", "{result}");
    assert_eq!(user.asked().len(), 1, "the user was asked once");
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "staging")),
        1,
        "the frontmost window holds it"
    );
}

#[gpui_kit::test]
async fn a_denied_connect_starts_nothing(cx: &mut TestAppContext) {
    let h = harness_with(cx, &["dev", "staging"], &["dev"]);
    let _ = workspace(cx, &["dev"]);
    let tools = ToolContext {
        approvals: Scripted::new(Script::Deny).gate(),
        ..h.tools.clone()
    };
    let mut agent = Agent::connect(cx, tools).await;

    let result = agent
        .call(cx, "connect_context", json!({"context": "staging"}))
        .await;

    assert_eq!(error_code(&result), "denied");
    assert!(
        cx.update(|cx| ClusterRegistry::existing_connection(cx, "staging"))
            .is_none(),
        "no session, tunnel or sign-in was started"
    );
}
