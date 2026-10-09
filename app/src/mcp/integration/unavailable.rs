//! An app that isn't running, through the adapter: the agent is told so in
//! a structured error rather than left hanging, and is served again once the
//! app is back.

use super::{Agent, content, error_code};
use crate::mcp::read::test_support::tools;
use gpui_kit::TestAppContext;
use serde_json::json;

#[gpui_kit::test]
async fn an_app_that_quits_is_reported_unavailable_until_it_is_back(cx: &mut TestAppContext) {
    let tools = tools(cx, &["dev"]);
    let mut agent = Agent::connect(cx, tools.clone()).await;
    let running = agent.call(cx, "list_contexts", json!({})).await;
    content(&running);

    agent.quit_app();

    let listed = agent.request(cx, "tools/list", json!({})).await;
    assert_eq!(listed["error"]["data"]["code"], "unavailable", "{listed}");
    assert_eq!(listed["error"]["data"]["reason"], "not_running");
    let called = agent
        .call(cx, "open_panel", json!({"context": "dev", "kind": "Pod"}))
        .await;
    assert_eq!(error_code(&called), "unavailable");
    assert_eq!(
        called["structuredContent"]["error"]["reason"],
        "not_running"
    );

    agent.relaunch_app(tools);

    let back = agent.call(cx, "list_contexts", json!({})).await;
    content(&back);
}
