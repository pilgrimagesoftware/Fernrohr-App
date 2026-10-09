//! Panel and saved-layout navigation through the adapter, into real
//! workspace windows (`navigate::test_support`).

use super::{Agent, content, error_code};
use crate::config::saved_layouts;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::mcp::navigate::test_support::{harness, open_targets, reopen, saved_layout, workspace};
use crate::ui::nav::NavTarget;
use gpui_kit::TestAppContext;
use serde_json::json;

#[gpui_kit::test]
async fn an_agent_opens_a_panel_in_the_window_holding_its_context(cx: &mut TestAppContext) {
    let h = harness(cx, &["dev", "prod"]);
    let (_dev_window, dev) = workspace(cx, &["dev"]);
    let (_prod_window, prod) = workspace(cx, &["prod"]);
    let dev_before = open_targets(cx, &dev);
    let mut agent = Agent::connect(cx, h.tools.clone()).await;

    let request = json!({"context": "prod", "kind": "events", "namespace": "team-a"});
    let first = agent.call(cx, "open_panel", request.clone()).await;
    let again = agent.call(cx, "open_panel", request).await;

    assert_eq!(content(&first)["created"], true, "{first}");
    assert_eq!(content(&again)["created"], false, "focused, not duplicated");
    assert_eq!(content(&first)["panel_id"], content(&again)["panel_id"]);
    assert!(open_targets(cx, &prod).contains(&NavTarget::Kind(DiscoveredKind::events())));
    assert_eq!(
        open_targets(cx, &dev),
        dev_before,
        "the other window is untouched"
    );
}

#[gpui_kit::test]
async fn an_agent_with_no_window_to_open_in_is_told_so(cx: &mut TestAppContext) {
    let h = harness(cx, &["dev"]);
    let mut agent = Agent::connect(cx, h.tools.clone()).await;

    let result = agent
        .call(cx, "open_panel", json!({"context": "dev", "kind": "Pod"}))
        .await;

    assert_eq!(error_code(&result), "ui_unavailable");
}

#[gpui_kit::test]
async fn an_agent_lists_and_loads_a_saved_layout_without_connecting_anything(
    cx: &mut TestAppContext,
) {
    let h = harness(cx, &["dev", "prod"]);
    let layout = saved_layout(cx, "Triage", &["dev", "prod"], &["prod"]);
    saved_layouts::save(&h.layouts_dir, &layout).expect("seeds a layout");
    // The layout's own window took both sessions when it closed.
    reopen(cx, &h, &["dev"]);
    let (_window, _dev) = workspace(cx, &["dev"]);
    let mut agent = Agent::connect(cx, h.tools.clone()).await;

    let listed = agent.call(cx, "list_layouts", json!({})).await;
    assert_eq!(
        content(&listed)["layouts"],
        json!([{"name": "Triage", "contexts": ["dev", "prod"]}])
    );

    let loaded = agent
        .call(
            cx,
            "load_layout",
            json!({"name": "triage", "mode": "replace"}),
        )
        .await;
    assert_eq!(content(&loaded)["name"], "Triage");
    assert_eq!(
        content(&loaded)["placeholders"],
        json!([{"context": "prod"}])
    );
    assert!(
        cx.update(|cx| ClusterRegistry::existing_connection(cx, "prod"))
            .is_none(),
        "loading connected no context"
    );

    let missing = agent
        .call(cx, "load_layout", json!({"name": "nope", "mode": "add"}))
        .await;
    assert_eq!(error_code(&missing), "unknown_layout");
}
