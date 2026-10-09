//! `fernrohr mcp`: the stdio MCP server an MCP client launches (design.md:
//! Stdio adapter with local app RPC).
//!
//! `rmcp` handles the protocol - initialization, version negotiation, JSON-RPC
//! framing, cancellation. This module only translates: `tools/list` and
//! `tools/call` become a [`Request`] to the running app through [`AppClient`],
//! and the [`Reply`] becomes an MCP result. It never starts the app; with none
//! running, every request gets a structured `unavailable` error.
//!
//! Tool failures are tool results with `isError` and a structured `error`
//! object, which a client shows its model. Only what MCP calls protocol
//! errors - no tool by that name, or no tool list to give - are JSON-RPC
//! errors, carrying the same object as their `data`.

use super::client::AppClient;
use super::endpoint::EndpointPaths;
use super::error::ToolError;
use super::protocol::{Reply, Request};
use super::tools::{ToolKind, ToolOutput, ToolSpec};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ErrorCode, ErrorData, Implementation,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServerHandler, ServiceExt};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};

/// Runs `fernrohr mcp` on this process's stdin and stdout until the client
/// closes them, and returns the exit code.
pub(super) fn run() -> i32 {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("fernrohr mcp: could not start: {error}");
            return 1;
        }
    };
    let client = AppClient::new(EndpointPaths::default_location());
    match runtime.block_on(serve(client, rmcp::transport::stdio())) {
        Ok(()) => 0,
        Err(error) => {
            // stdout is the protocol stream; diagnostics go to stderr, which
            // MCP clients collect as the server's log.
            eprintln!("fernrohr mcp: {error}");
            1
        }
    }
}

/// Serves MCP over `transport` - stdio in production, an in-memory pipe in
/// tests - forwarding to the app behind `client`.
pub(super) async fn serve<R, W>(client: AppClient, transport: (R, W)) -> Result<(), String>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let running = Adapter { client }
        .serve(transport)
        .await
        .map_err(|error| error.to_string())?;
    running.waiting().await.map_err(|error| error.to_string())?;
    Ok(())
}

struct Adapter {
    client: AppClient,
}

impl ServerHandler for Adapter {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("fernrohr", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Reads the Kubernetes clusters connected in the running Fernrohr app and \
                 shows resources in its panels. Fernrohr must be running.",
            )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        match self.client.request(&Request::ListTools).await {
            Ok(Reply::Tools { tools }) => Ok(ListToolsResult::with_all_items(
                tools.into_iter().map(mcp_tool).collect(),
            )),
            Ok(Reply::Called { .. }) => Err(protocol_error(&ToolError::Internal)),
            Err(error) => Err(protocol_error(&error)),
        }
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let request = Request::CallTool {
            name: request.name.into_owned(),
            arguments: request.arguments.unwrap_or_default(),
        };
        let result = match self.client.request(&request).await {
            Ok(Reply::Called { result }) => result,
            Ok(Reply::Tools { .. }) => Err(ToolError::Internal),
            Err(error) => Err(error),
        };
        match result {
            Ok(output) => Ok(CallToolResult::structured(output_json(output)).into()),
            Err(error @ ToolError::UnknownTool { .. }) => Err(ErrorData::invalid_params(
                error.to_string(),
                Some(error.to_json()),
            )),
            Err(error) => {
                Ok(CallToolResult::structured_error(json!({ "error": error.to_json() })).into())
            }
        }
    }
}

/// A tool as MCP lists it. Its kind becomes the read-only hint; the hint is
/// advice to the client, never the authority - actions are gated in the app.
fn mcp_tool(spec: ToolSpec) -> Tool {
    let annotations = ToolAnnotations::new()
        .read_only(spec.kind != ToolKind::Action)
        .destructive(spec.kind == ToolKind::Action);
    Tool::new(spec.name, spec.description, Arc::new(spec.input_schema))
        .with_title(spec.title)
        .with_annotations(annotations)
}

/// A result's structured content: the tool's object, flagged when the tool
/// cut it short.
fn output_json(output: ToolOutput) -> Value {
    let mut content = output.content;
    if output.truncated {
        content.insert("truncated".to_string(), Value::Bool(true));
    }
    Value::Object(content)
}

fn protocol_error(error: &ToolError) -> ErrorData {
    ErrorData::new(
        ErrorCode::INTERNAL_ERROR,
        error.to_string(),
        Some(error.to_json()),
    )
}

#[cfg(test)]
mod tests;
