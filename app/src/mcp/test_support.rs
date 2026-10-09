//! Fixtures shared by the endpoint, server, adapter and integration tests: a
//! fixture tool, a scratch endpoint directory, an app endpoint served
//! in-process, and an MCP client driving a real adapter ([`McpClient`]).

use super::endpoint::{Endpoint, EndpointFiles, EndpointPaths};
use super::foreground::Foreground;
use super::server::{ServerState, serve};
use super::tools::{ToolContext, ToolKind, ToolOutput, ToolRegistry, ToolSpec};
use crate::k8s::cluster::discovery::DiscoveredKind;
use kube::api::GroupVersionKind;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::task::JoinHandle;

mod mcp_client;

pub(super) use mcp_client::McpClient;

/// A fresh, empty endpoint directory for one test. Short, since a socket path
/// has to fit `sun_path` (104 bytes on macOS).
pub(super) fn temp_endpoint_paths() -> EndpointPaths {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("fr-mcp-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    EndpointPaths::in_dir(dir)
}

/// The fixture tool: echoes `text` back.
pub(super) fn echo_spec() -> ToolSpec {
    ToolSpec {
        name: "echo".into(),
        title: "Echo".into(),
        description: "Returns its text.".into(),
        kind: ToolKind::Read,
        input_schema: object(json!({
            "type": "object",
            "properties": {"text": {"type": "string"}},
            "required": ["text"],
        })),
    }
}

#[derive(Deserialize)]
struct EchoInput {
    text: String,
}

/// A registry holding only [`echo_spec`]'s tool.
pub(super) fn echo_tool_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::default();
    registry.register(echo_spec(), |input: EchoInput, _| async move {
        Ok(ToolOutput::new(object(json!({"echo": input.text}))))
    });
    registry
}

/// A tool context with no main thread behind it.
pub(super) fn test_context() -> ToolContext {
    let (foreground, _jobs) = Foreground::channel();
    ToolContext {
        foreground,
        kubeconfig: None,
        approvals: super::approval::scripted::Scripted::deny().gate(),
        tunnels: Some(crate::util::test_paths::temp_path("mcp-tunnels")),
        connect_settle: crate::consts::MCP_CONNECT_SETTLE,
    }
}

pub(super) fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("{other} is not an object"),
    }
}

/// An app endpoint served on the current tokio runtime until dropped.
pub(super) struct TestServer {
    pub(super) files: EndpointFiles,
    task: JoinHandle<()>,
}

impl TestServer {
    /// Binds `paths` and serves `registry` there, with no main thread behind
    /// its tools ([`test_context`]).
    pub(super) fn start(paths: EndpointPaths, registry: ToolRegistry) -> Self {
        Self::start_with(paths, registry, test_context())
    }

    /// [`Self::start`] with the tools' context given: a real main thread, a
    /// kubeconfig and an approver, for tests that drive the app's own tools
    /// through the endpoint.
    pub(super) fn start_with(
        paths: EndpointPaths,
        registry: ToolRegistry,
        context: ToolContext,
    ) -> Self {
        let endpoint = Endpoint::bind(paths).expect("the test endpoint binds");
        let state = Arc::new(ServerState {
            registry,
            token: endpoint.files.token.clone(),
            owner_uid: endpoint.owner_uid,
            context,
        });
        let files = endpoint.files.clone();
        let task = tokio::spawn(serve(endpoint.listener, state));
        Self { files, task }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
        self.files.remove();
    }
}

/// The directory a test endpoint lives in, for assertions on it.
pub(super) fn dir_of(paths: &EndpointPaths) -> PathBuf {
    paths
        .socket
        .parent()
        .expect("an endpoint socket has a directory")
        .to_path_buf()
}

/// A discovered kind with every verb, as a fixture's discovery reports it.
pub(super) fn discovered(
    group: &str,
    version: &str,
    kind: &str,
    plural: &str,
    namespaced: bool,
) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk(group, version, kind),
        plural: plural.to_string(),
        namespaced,
        verbs: Default::default(),
    }
}
