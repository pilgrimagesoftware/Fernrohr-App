//! `get_pod_logs`: a bounded snapshot of one container's logs - never a
//! stream (design.md Non-Goals: streaming watch subscriptions).
//!
//! Bounded twice. The cluster is asked for at most the newest `tail_lines`
//! lines and [`MCP_LOG_READ_CAP`] bytes, so a container with enormous lines
//! can't make the app buffer without limit; of what arrives, the newest
//! `limit_bytes` are kept, from a line start, and the result says when older
//! output was dropped.

use super::shape::{newest_bytes, output};
use crate::consts::{
    MCP_LOG_DEFAULT_BYTES, MCP_LOG_DEFAULT_TAIL_LINES, MCP_LOG_MAX_TAIL_LINES, MCP_LOG_READ_CAP,
    MCP_RESULT_BUDGET_BYTES,
};
use crate::mcp::cluster::session;
use crate::mcp::error::ToolError;
use crate::mcp::names::{LabelName, ObjectName};
use crate::mcp::tools::{ToolContext, ToolKind, ToolRegistry, ToolResult};
use futures_util::AsyncReadExt;
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::api::LogParams;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

pub(super) fn register(registry: &mut ToolRegistry) {
    registry.add(
        "get_pod_logs",
        "Get pod logs",
        "Returns the newest log lines of one container in a Pod, from a \
         connected context. Bounded by `tail_lines` and `limit_bytes`; when \
         older output was dropped to fit, the result is marked `truncated`.",
        ToolKind::Read,
        get_pod_logs,
    );
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct GetPodLogsInput {
    /// The context's name, as `list_contexts` reports it.
    context: String,
    /// The Pod's namespace.
    namespace: String,
    /// The Pod's name.
    pod: String,
    /// The container to read; needed only when the Pod has several.
    #[serde(default)]
    container: Option<String>,
    /// Read the previous, terminated instance of the container - after a
    /// crash or restart.
    #[serde(default)]
    previous: bool,
    /// How many of the newest lines to read, 1 to 5000; 200 when omitted.
    #[serde(default)]
    tail_lines: Option<i64>,
    /// The most bytes of logs to return, 1 to 1048576; 262144 when omitted.
    #[serde(default)]
    limit_bytes: Option<usize>,
}

async fn get_pod_logs(input: GetPodLogsInput, tools: ToolContext) -> ToolResult {
    let namespace = LabelName::parse("namespace", &input.namespace)?;
    let pod = ObjectName::parse("pod", &input.pod)?;
    let container = input
        .container
        .as_deref()
        .map(|container| LabelName::parse("container", container))
        .transpose()?;
    let tail_lines = input.tail_lines.unwrap_or(MCP_LOG_DEFAULT_TAIL_LINES);
    if !(1..=MCP_LOG_MAX_TAIL_LINES).contains(&tail_lines) {
        return Err(out_of_range("tail_lines", MCP_LOG_MAX_TAIL_LINES as usize));
    }
    let limit_bytes = input.limit_bytes.unwrap_or(MCP_LOG_DEFAULT_BYTES);
    if !(1..=MCP_RESULT_BUDGET_BYTES).contains(&limit_bytes) {
        return Err(out_of_range("limit_bytes", MCP_RESULT_BUDGET_BYTES));
    }

    let session = session(&tools, &input.context).await?;
    let params = LogParams {
        container: container.as_ref().map(|name| name.as_str().to_string()),
        previous: input.previous,
        tail_lines: Some(tail_lines),
        limit_bytes: Some(MCP_LOG_READ_CAP as i64),
        follow: false,
        ..LogParams::default()
    };
    let api: Api<Pod> = Api::namespaced(session.client.clone(), namespace.as_str());
    let kube_error = |error: kube::Error| ToolError::from_kube(&session.context, &error);
    let stream = api
        .log_stream(pod.as_str(), &params)
        .await
        .map_err(kube_error)?;
    let mut log = Vec::new();
    stream
        .take(MCP_LOG_READ_CAP as u64)
        .read_to_end(&mut log)
        .await
        .map_err(|_| ToolError::ConnectionFailed {
            context: session.context.clone(),
        })?;

    let (logs, truncated) = newest_bytes(&log, limit_bytes);
    let mut result = output(json!({
        "context": session.context,
        "namespace": namespace.as_str(),
        "pod": pod.as_str(),
        "container": container.as_ref().map(LabelName::as_str),
        "previous": input.previous,
        "logs": logs,
    }));
    result.truncated = truncated;
    Ok(result)
}

fn out_of_range(field: &str, max: usize) -> ToolError {
    ToolError::InvalidArguments {
        message: format!("`{field}` must be between 1 and {max}"),
    }
}

#[cfg(test)]
mod tests;
