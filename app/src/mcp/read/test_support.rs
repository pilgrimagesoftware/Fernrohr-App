//! Running the read tools against fixture sessions: a GPUI app with the
//! tokio runtime, contexts registered as already connected (or not), and a
//! kubeconfig of the test's own.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::discovery_registry::DiscoveryRegistry;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::mcp::foreground::Foreground;
use crate::mcp::test_support::object;
use crate::mcp::tools::{ToolContext, ToolRegistry, ToolResult};
use gpui_kit::TestAppContext;
use serde_json::Value;
use std::path::PathBuf;

/// The app's tool context for a test whose kubeconfig lists `contexts`.
/// Starts the runtime and lets the test wait on it.
pub(in crate::mcp) fn tools(cx: &mut TestAppContext, contexts: &[&str]) -> ToolContext {
    // Tool calls run on the tokio runtime and cross back to this thread;
    // GPUI's deterministic scheduler forbids that wait by default.
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    ToolContext {
        foreground: cx.update(Foreground::spawn_on),
        kubeconfig: Some(kubeconfig_with(contexts)),
    }
}

/// Registers `context` as an open session in `state`, with `kinds` as its
/// discovery when given.
pub(in crate::mcp) fn open(
    cx: &mut TestAppContext,
    context: &str,
    state: ConnectionState,
    kinds: Option<Vec<DiscoveredKind>>,
) {
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, context, state);
        if let Some(kinds) = kinds {
            DiscoveryRegistry::insert_test(cx, context, kinds);
        }
    });
}

/// Calls the app's tool `name` with `arguments` as a client would.
pub(in crate::mcp) async fn call(
    cx: &mut TestAppContext,
    tools: &ToolContext,
    name: &str,
    arguments: Value,
) -> ToolResult {
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let call = ToolRegistry::app().call(name, object(arguments), tools.clone());
    handle.spawn(call).await.expect("the tool call runs")
}

/// A kubeconfig file defining `contexts`, all on one unreachable cluster.
fn kubeconfig_with(contexts: &[&str]) -> PathBuf {
    let path = crate::util::test_paths::temp_path("mcp-kubeconfig");
    let entries: String = contexts
        .iter()
        .map(|name| format!("- name: {name}\n  context: {{cluster: c, user: u}}\n"))
        .collect();
    let yaml = format!(
        "apiVersion: v1\nkind: Config\n\
         clusters:\n- name: c\n  cluster: {{server: \"https://127.0.0.1:1\"}}\n\
         users:\n- name: u\n  user: {{}}\n\
         contexts:\n{entries}"
    );
    std::fs::write(&path, yaml).expect("the test kubeconfig is written");
    path
}
