//! The navigation tools end to end: each call is made through the app's
//! [`ToolRegistry`] on the tokio runtime, and crosses to GPUI's main thread
//! through a real [`Foreground`] into real workspace windows - the same path
//! an MCP client's request takes after the endpoint.
//!
//! Windows and sessions are `navigate::test_support`'s: connected to a
//! recording fake API server, with the core kinds as their discovery, so
//! nothing here dials a cluster.
//!
//! Named imports rather than `use super::*` plus a `gpui_kit::*` glob: see
//! `util/shell/open/tests.rs` on the macro-expansion budget.

use super::super::tools::{ToolRegistry, ToolResult};
use super::test_support::{
    Harness, harness, harness_with, open_targets, reopen, saved_layout, workspace,
};
use crate::config::saved_layouts;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::mcp::error::ToolError;
use crate::ui::nav::NavTarget;
use gpui_kit::TestAppContext;
use serde_json::{Map, Value, json};

/// Calls `name` the way the endpoint does: from a task on the tokio runtime.
async fn call(cx: &mut TestAppContext, h: &Harness, name: &str, arguments: Value) -> ToolResult {
    let Value::Object(arguments) = arguments else {
        panic!("tool arguments are an object");
    };
    let call = ToolRegistry::app().call(name, arguments, h.tools.clone());
    let runtime = cx.update(|cx| crate::runtime::handle(cx));
    let result = runtime.spawn(call).await.expect("the call ran");
    cx.run_until_parked();
    result
}

fn content(result: ToolResult) -> Map<String, Value> {
    result.expect("the tool succeeded").content
}

#[gpui_kit::test]
async fn open_panel_opens_in_the_window_holding_the_context_and_leaves_others_alone(
    cx: &mut TestAppContext,
) {
    let h = harness(cx, &["dev", "prod"]);
    let (_dev_window, dev) = workspace(cx, &["dev"]);
    let (_prod_window, prod) = workspace(cx, &["prod"]);
    let dev_before = open_targets(cx, &dev);

    let opened = content(
        call(
            cx,
            &h,
            "open_panel",
            json!({"context": "prod", "kind": "events", "namespace": "team-a"}),
        )
        .await,
    );

    assert_eq!(opened["created"], json!(true));
    let panel_id = opened["panel_id"].as_str().expect("an opaque string id");
    assert!(panel_id.starts_with("panel-"), "{panel_id}");
    assert!(
        open_targets(cx, &prod).contains(&NavTarget::Kind(DiscoveredKind::events())),
        "the Events list opened where `prod` is held"
    );
    assert_eq!(
        open_targets(cx, &dev),
        dev_before,
        "the other window is untouched"
    );
}

#[gpui_kit::test]
async fn open_panel_focuses_the_panel_it_already_opened(cx: &mut TestAppContext) {
    let h = harness(cx, &["dev"]);
    let (_window, dev) = workspace(cx, &["dev"]);
    let request = json!({"context": "dev", "kind": "Pod", "namespace": "team-a", "name": "api-0"});

    let first = content(call(cx, &h, "open_panel", request.clone()).await);
    let panels = open_targets(cx, &dev).len();
    let second = content(call(cx, &h, "open_panel", request).await);

    assert_eq!(first["created"], json!(true));
    assert_eq!(second["created"], json!(false));
    assert_eq!(
        first["panel_id"], second["panel_id"],
        "the same panel, focused"
    );
    assert_eq!(open_targets(cx, &dev).len(), panels, "no duplicate");
    assert!(open_targets(cx, &dev).contains(&NavTarget::pod("team-a", "api-0")));
}

#[gpui_kit::test]
async fn open_panel_refuses_what_it_cannot_show(cx: &mut TestAppContext) {
    let h = harness_with(cx, &["dev", "prod", "stray"], &["dev", "stray"]);
    let (_window, dev) = workspace(cx, &["dev"]);
    let before = open_targets(cx, &dev);

    let refusals = [
        (
            json!({"context": "nope", "kind": "Pod"}),
            ToolError::UnknownContext {
                context: "nope".into(),
            },
        ),
        // Known, but the user hasn't opened it.
        (
            json!({"context": "prod", "kind": "Pod"}),
            ToolError::Disconnected {
                context: "prod".into(),
            },
        ),
        // Open and connected, but no workspace window holds it.
        (
            json!({"context": "stray", "kind": "Pod"}),
            ToolError::Disconnected {
                context: "stray".into(),
            },
        ),
        (
            json!({"context": "dev", "kind": "Fern"}),
            ToolError::UnsupportedKind {
                kind: "Fern".into(),
            },
        ),
    ];
    for (request, expected) in refusals {
        assert_eq!(
            call(cx, &h, "open_panel", request.clone()).await,
            Err(expected),
            "{request}"
        );
    }
    let stray = call(
        cx,
        &h,
        "open_panel",
        json!({"context": "dev", "kind": "Pod", "x": 1}),
    )
    .await;
    assert!(matches!(stray, Err(ToolError::InvalidArguments { .. })));

    assert_eq!(open_targets(cx, &dev), before);
    assert!(
        cx.update(|cx| ClusterRegistry::existing_connection(cx, "prod"))
            .is_none(),
        "refusing `prod` started no session for it"
    );
}

/// kube joins a name onto the request path unescaped, so a name carrying `?`
/// or `#` would smuggle a query or fragment into whatever the panel requests.
#[gpui_kit::test]
async fn open_panel_refuses_a_name_that_would_change_the_request_path(cx: &mut TestAppContext) {
    let h = harness(cx, &["dev"]);
    let (_window, dev) = workspace(cx, &["dev"]);
    let before = open_targets(cx, &dev);
    for name in ["x?watch=true", "x#frag", "../secrets/db", "a%2Fb"] {
        let result = call(
            cx,
            &h,
            "open_panel",
            json!({"context": "dev", "kind": "Pod", "namespace": "team-a", "name": name}),
        )
        .await;
        assert!(
            matches!(result, Err(ToolError::InvalidArguments { .. })),
            "{name}: {result:?}"
        );
    }
    let namespace = call(
        cx,
        &h,
        "open_panel",
        json!({"context": "dev", "kind": "Pod", "namespace": "a?b"}),
    )
    .await;
    assert!(matches!(namespace, Err(ToolError::InvalidArguments { .. })));
    assert_eq!(open_targets(cx, &dev), before, "nothing opened");
}

#[gpui_kit::test]
async fn open_panel_with_no_window_reports_the_ui_unavailable(cx: &mut TestAppContext) {
    let h = harness(cx, &["dev"]);
    let result = call(
        cx,
        &h,
        "open_panel",
        json!({"context": "dev", "kind": "Pod"}),
    )
    .await;
    assert_eq!(result, Err(ToolError::UiUnavailable));
}

#[gpui_kit::test]
async fn list_layouts_names_every_saved_layout(cx: &mut TestAppContext) {
    let h = harness(cx, &["dev"]);
    for name in ["Triage", "Deploys"] {
        let layout = saved_layout(cx, name, &["dev"], &[]);
        saved_layouts::save(&h.layouts_dir, &layout).expect("seeds a layout");
    }
    let listed = content(call(cx, &h, "list_layouts", json!({})).await);
    assert_eq!(
        listed["layouts"],
        json!([
            {"name": "Deploys", "contexts": ["dev"]},
            {"name": "Triage", "contexts": ["dev"]},
        ])
    );
}

#[gpui_kit::test]
async fn load_layout_restores_a_missing_context_panel_as_a_reported_placeholder(
    cx: &mut TestAppContext,
) {
    let h = harness(cx, &["dev", "prod"]);
    let layout = saved_layout(cx, "Triage", &["dev", "prod"], &["prod"]);
    saved_layouts::save(&h.layouts_dir, &layout).expect("seeds a layout");
    reopen(cx, &h, &["dev"]);
    let (_window, dev) = workspace(cx, &["dev"]);
    let prod_session = |cx: &mut TestAppContext| {
        cx.update(|cx| ClusterRegistry::existing_connection(cx, "prod").is_some())
    };
    assert!(
        !prod_session(cx),
        "the layout's own window took `prod` with it"
    );

    let loaded = content(
        call(
            cx,
            &h,
            "load_layout",
            json!({"name": "triage", "mode": "add"}),
        )
        .await,
    );

    assert_eq!(loaded["name"], json!("Triage"));
    assert_eq!(loaded["placeholders"], json!([{"context": "prod"}]));
    let names: Vec<&str> = cx.update(|cx| {
        dev.read(cx)
            .test_dock_views(cx)
            .iter()
            .map(|view| view.panel_name(cx))
            .collect()
    });
    assert!(
        names.contains(&crate::ui::unrestored::PANEL_NAME),
        "the `prod` Events panel restored as a placeholder: {names:?}"
    );
    assert!(!prod_session(cx), "loading started no session for `prod`");
}

#[gpui_kit::test]
async fn load_layout_replace_swaps_the_arrangement(cx: &mut TestAppContext) {
    let h = harness(cx, &["dev"]);
    let layout = saved_layout(cx, "Pods only", &["dev"], &[]);
    saved_layouts::save(&h.layouts_dir, &layout).expect("seeds a layout");
    reopen(cx, &h, &["dev"]);
    let (_window, dev) = workspace(cx, &["dev"]);
    content(
        call(
            cx,
            &h,
            "open_panel",
            json!({"context": "dev", "kind": "events"}),
        )
        .await,
    );
    assert!(open_targets(cx, &dev).contains(&NavTarget::Kind(DiscoveredKind::events())));

    let loaded = content(
        call(
            cx,
            &h,
            "load_layout",
            json!({"name": "Pods only", "mode": "replace"}),
        )
        .await,
    );

    assert_eq!(loaded["placeholders"], json!([]));
    assert_eq!(open_targets(cx, &dev), vec![NavTarget::pods()]);
}

#[gpui_kit::test]
async fn load_layout_names_a_layout_that_does_not_exist(cx: &mut TestAppContext) {
    let h = harness(cx, &["dev"]);
    let (_window, _dev) = workspace(cx, &["dev"]);
    let result = call(
        cx,
        &h,
        "load_layout",
        json!({"name": "nope", "mode": "add"}),
    )
    .await;
    assert_eq!(
        result,
        Err(ToolError::UnknownLayout {
            name: "nope".into()
        })
    );
}

#[test]
fn the_navigation_tools_are_navigate_tools() {
    use super::super::tools::ToolKind;
    let specs = ToolRegistry::app().specs();
    for name in [super::OPEN_PANEL, super::LIST_LAYOUTS, super::LOAD_LAYOUT] {
        let spec = specs
            .iter()
            .find(|spec| spec.name == name)
            .unwrap_or_else(|| panic!("{name} is registered"));
        assert_eq!(spec.kind, ToolKind::Navigate, "{name}");
        assert_eq!(spec.input_schema["type"], "object", "{name}");
        assert!(!spec.input_schema.contains_key("title"), "{name}");
    }
}

mod focus;
