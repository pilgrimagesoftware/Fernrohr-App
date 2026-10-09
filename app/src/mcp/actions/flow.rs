//! The steps every action tool takes, in order, so none can skip one:
//!
//! 1. its input has already parsed - kinds from the tool's own allowlist,
//!    names as safe path segments (`inputs`);
//! 2. [`resolve`] finds the open session and the kind in its discovery, and
//!    checks it is exactly the allowed kind - no request yet;
//! 3. the tool reads what the dialog has to show (current replicas, the old
//!    value, the revision to go back to);
//! 4. `approval::approve` asks the user - denied, timed out or withdrawn,
//!    the tool stops here with no write sent;
//! 5. the tool calls its shared `resource_actions` function, and
//!    [`approved`] reports what changed.

use super::inputs::Target;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::resource_actions::ActionError;
use crate::mcp::cluster::{Session, session};
use crate::mcp::error::ToolError;
use crate::mcp::kinds::KindQuery;
use crate::mcp::tools::{ToolContext, ToolOutput};
use serde_json::{Map, Value, json};

/// The longest a value is shown in the approval dialog, in characters.
const PREVIEW_CHARS: usize = 200;

/// `target`'s session and the kind `query` names in it, which must be
/// exactly `query`'s kind and group.
pub(super) async fn resolve(
    tools: &ToolContext,
    target: &Target,
    query: KindQuery,
) -> Result<(Session, DiscoveredKind), ToolError> {
    let session = session(tools, &target.context).await?;
    let kind = session.kind(tools, &query).await?;
    let group = match query.group.as_deref() {
        Some("core") | None => "",
        Some(group) => group,
    };
    if kind.gvk.kind != query.kind || kind.gvk.group != group {
        return Err(ToolError::UnsupportedKind { kind: query.kind });
    }
    Ok((session, kind))
}

/// A shared action's failure, for `session`'s context.
pub(super) fn failed(session: &Session) -> impl Fn(ActionError) -> ToolError + '_ {
    move |error| ToolError::from_action(&session.context, &error)
}

/// How a kind reads in the dialog: `Deployment (apps)`, or `Pod`.
pub(super) fn kind_label(kind: &DiscoveredKind) -> String {
    if kind.gvk.group.is_empty() {
        kind.gvk.kind.clone()
    } else {
        format!("{} ({})", kind.gvk.kind, kind.gvk.group)
    }
}

/// A value as the dialog shows it: whole when short, else its start and how
/// long it is.
pub(super) fn preview(value: Option<&str>, absent: &str) -> String {
    match value {
        None => absent.to_string(),
        Some(value) if value.chars().count() <= PREVIEW_CHARS => format!("{value:?}"),
        Some(value) => {
            let start: String = value.chars().take(PREVIEW_CHARS).collect();
            format!("{start:?}… ({} bytes)", value.len())
        }
    }
}

/// An approved action's result: `"outcome": "approved"`, then `fields`.
pub(super) fn approved(fields: Value) -> ToolOutput {
    let mut content = Map::from_iter([("outcome".to_string(), json!("approved"))]);
    if let Value::Object(fields) = fields {
        content.extend(fields);
    }
    ToolOutput::new(content)
}
