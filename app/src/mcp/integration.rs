//! `agent-mcp` as an agent sees it (expose-cluster-mcp 5.1): every test here
//! speaks MCP, as newline-delimited JSON-RPC, to a real `fernrohr mcp` adapter
//! ([`McpClient`]), which forwards over a real endpoint socket to the app's own
//! [`ToolRegistry::app`] - served with a real GPUI main thread, approver and
//! kubeconfig. Clusters are recording fakes (`k8s::test_recorder`,
//! `k8s::test_cluster`), so nothing needs a live cluster.
//!
//! Each tool's own tests pin its behaviour in detail; these check that it
//! survives the whole trip: [`reads`], [`actions`] (approved, denied and timed
//! out), [`navigation`] into real windows, and an app that isn't
//! [`unavailable`] or stops being so.

mod actions;
mod navigation;
mod reads;
mod unavailable;

use super::endpoint::EndpointPaths;
use super::test_support::{McpClient, TestServer, temp_endpoint_paths};
use super::tools::{ToolContext, ToolRegistry};
use gpui_kit::TestAppContext;
use serde_json::{Value, json};
use tokio::runtime::Handle;

/// An agent connected to a running app through the adapter.
struct Agent {
    /// Taken while a request is in flight on the runtime.
    client: Option<McpClient>,
    runtime: Handle,
    paths: EndpointPaths,
    /// The app's endpoint: `None` once the app has quit.
    app: Option<TestServer>,
}

impl Agent {
    /// Starts the app's endpoint with `tools` as its tools' context, and an
    /// agent whose adapter has completed MCP initialization with it.
    async fn connect(cx: &mut TestAppContext, tools: ToolContext) -> Self {
        let runtime = cx.update(|cx| crate::runtime::handle(cx));
        let paths = temp_endpoint_paths();
        let app = serve_app(&runtime, paths.clone(), tools);
        let client = runtime
            .spawn(McpClient::connect(paths.clone()))
            .await
            .expect("the adapter starts");
        Self {
            client: Some(client),
            runtime,
            paths,
            app: Some(app),
        }
    }

    /// Sends one MCP request and returns the whole response. It runs on the
    /// tokio runtime, as a real adapter does, while this thread keeps GPUI's
    /// main thread turning for the tools that need it.
    async fn request(
        &mut self,
        cx: &mut TestAppContext,
        method: &'static str,
        params: Value,
    ) -> Value {
        let mut client = self.client.take().expect("one request at a time");
        let (client, response) = self
            .runtime
            .spawn(async move {
                let response = client.request(method, params).await;
                (client, response)
            })
            .await
            .expect("the request ran");
        self.client = Some(client);
        cx.run_until_parked();
        response
    }

    /// Calls the tool `name` and returns its MCP `result`.
    async fn call(&mut self, cx: &mut TestAppContext, name: &str, arguments: Value) -> Value {
        let response = self
            .request(
                cx,
                "tools/call",
                json!({"name": name, "arguments": arguments}),
            )
            .await;
        assert!(response["error"].is_null(), "{name}: {response}");
        response["result"].clone()
    }

    /// The app quits: its endpoint goes away.
    fn quit_app(&mut self) {
        self.app = None;
    }

    /// The app starts again where the adapter expects it.
    fn relaunch_app(&mut self, tools: ToolContext) {
        self.app = Some(serve_app(&self.runtime, self.paths.clone(), tools));
    }
}

/// The app's endpoint at `paths`, serving its own tools, on the app runtime.
fn serve_app(runtime: &Handle, paths: EndpointPaths, tools: ToolContext) -> TestServer {
    // Binding a socket and spawning its server need the runtime's context.
    let _inside = runtime.enter();
    TestServer::start_with(paths, ToolRegistry::app(), tools)
}

/// A successful result's structured content.
fn content(result: &Value) -> &Value {
    assert_eq!(result["isError"], false, "{result}");
    &result["structuredContent"]
}

/// A failed result's error code.
fn error_code(result: &Value) -> &str {
    assert_eq!(result["isError"], true, "{result}");
    result["structuredContent"]["error"]["code"]
        .as_str()
        .unwrap_or_else(|| panic!("no error code in {result}"))
}
