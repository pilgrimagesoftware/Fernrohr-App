//! The navigation tools (`agent-mcp`: Panel navigation tool, Saved layout
//! tools; Fernrohr#189 section 4): `open_panel`, `list_layouts` and
//! `load_layout`. They change what the app shows, never a cluster, so they are
//! [`ToolKind::Navigate`] and need no confirmation.
//!
//! Each runs on the tokio runtime and crosses to the main thread through
//! [`Foreground`](super::foreground::Foreground) for the window work, which
//! `util::shell`'s `open_panel`/`load_layout` do through the app's own open and
//! load paths. `open_panel` finds its context and kind the way the read tools
//! do (`cluster::session`, `kinds::resolve`), so it never starts a connection.
//! Reading the layouts directory happens on a blocking thread, so the main
//! thread does no file I/O for a tool.

mod request;

use super::cluster::session;
use super::error::ToolError;
use super::kinds::KindQuery;
use super::names::{LabelName, ObjectName};
use super::tools::{ToolContext, ToolKind, ToolOutput, ToolRegistry, ToolResult};
use crate::config::saved_layouts::{self, SavedLayout};
use crate::util::shell::{self, LoadMode, NavigateError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::path::PathBuf;

const OPEN_PANEL: &str = "open_panel";
const LIST_LAYOUTS: &str = "list_layouts";
const LOAD_LAYOUT: &str = "load_layout";

/// Adds the navigation tools to `registry`.
pub(super) fn register(registry: &mut ToolRegistry) {
    registry.add(
        OPEN_PANEL,
        "Open panel",
        "Opens a resource panel in Fernrohr, or focuses it if that window already shows \
         it, in the frontmost window connected to the context. Without `name` it opens \
         the kind's list; with `name` it opens that object's detail panel. Returns an \
         opaque panel ID.",
        ToolKind::Navigate,
        open_panel,
    );
    registry.add(
        LIST_LAYOUTS,
        "List saved layouts",
        "Lists the user's saved Fernrohr layouts by name, with the contexts each was \
         saved with.",
        ToolKind::Navigate,
        list_layouts,
    );
    registry.add(
        LOAD_LAYOUT,
        "Load saved layout",
        "Loads a saved layout into the frontmost Fernrohr window, as its Load Layout \
         command does: `replace` swaps the window's arrangement for the layout's, `add` \
         opens the layout's panels beside the ones already open. A panel for a context \
         the window isn't connected to restores as a placeholder, listed in \
         `placeholders`; no context is connected.",
        ToolKind::Navigate,
        load_layout,
    );
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct OpenPanelInput {
    /// The context's name, as `list_contexts` reports it; a Fernrohr window
    /// must hold it.
    context: String,
    /// The resource kind: its name, plural or singular.
    kind: String,
    /// The kind's API group, needed only when two groups have a kind of that
    /// name: `apps`, or `core` for the core group.
    #[serde(default)]
    group: Option<String>,
    /// Scope a namespaced list to this namespace (omit for the context's
    /// default scope); required with `name` for a namespaced kind.
    #[serde(default)]
    namespace: Option<String>,
    /// Open this object's detail panel instead of the kind's list.
    #[serde(default)]
    name: Option<String>,
}

async fn open_panel(input: OpenPanelInput, tools: ToolContext) -> ToolResult {
    let namespace = input
        .namespace
        .as_deref()
        .map(|namespace| LabelName::parse("namespace", namespace))
        .transpose()?;
    let name = input
        .name
        .as_deref()
        .map(|name| ObjectName::parse("name", name))
        .transpose()?;
    let session = session(&tools, &input.context).await?;
    let query = KindQuery {
        kind: input.kind,
        group: input.group,
    };
    let kind = session.kind(&tools, &query).await?;
    let (target, namespaces) = request::panel_target(&kind, namespace, name)?;
    let context = input.context;
    let shown = tools
        .foreground
        .run(move |cx| {
            shell::open_panel(context.clone(), target, namespaces, cx).map_err(|error| {
                match error {
                    NavigateError::NoWindow => ToolError::UiUnavailable,
                    // Connected, but only a window that isn't a workspace (or
                    // none) holds it.
                    NavigateError::ContextNotHeld => ToolError::Disconnected { context },
                }
            })
        })
        .await??;
    Ok(ToolOutput::new(object(json!({
        "panel_id": panel_id(shown.id.as_u64()),
        "created": shown.created,
    }))))
}

/// A panel's ID as a client sees it. Opaque: stable while the panel stays
/// open, and meaningless to anything but this app run.
fn panel_id(raw: u64) -> String {
    format!("panel-{raw}")
}

/// `list_layouts` takes no arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ListLayoutsInput {}

async fn list_layouts(_: ListLayoutsInput, tools: ToolContext) -> ToolResult {
    let layouts: Vec<Value> = read_layouts(&tools)
        .await?
        .into_iter()
        .map(|layout| json!({"name": layout.name, "contexts": layout.contexts}))
        .collect();
    Ok(ToolOutput::new(object(json!({"layouts": layouts}))))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct LoadLayoutInput {
    /// The saved layout's name, as `list_layouts` gives it (any case).
    name: String,
    /// `replace` swaps the window's arrangement; `add` opens the layout's
    /// panels beside the ones already open.
    mode: LayoutMode,
}

/// `load_layout`'s `mode`, as the client spells it.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum LayoutMode {
    Add,
    Replace,
}

impl From<LayoutMode> for LoadMode {
    fn from(mode: LayoutMode) -> Self {
        match mode {
            LayoutMode::Add => Self::Add,
            LayoutMode::Replace => Self::Replace,
        }
    }
}

async fn load_layout(input: LoadLayoutInput, tools: ToolContext) -> ToolResult {
    let layout = read_layouts(&tools)
        .await?
        .into_iter()
        .find(|layout| layout.name.eq_ignore_ascii_case(&input.name))
        .ok_or_else(|| ToolError::UnknownLayout {
            name: input.name.clone(),
        })?;
    let name = layout.name.clone();
    let mode = LoadMode::from(input.mode);
    let loaded = tools
        .foreground
        .run(move |cx| shell::load_layout(layout, mode, cx))
        .await?
        .ok_or(ToolError::UiUnavailable)?;
    let placeholders: Vec<Value> = loaded
        .placeholder_contexts
        .into_iter()
        .map(|context| json!({"context": context}))
        .collect();
    Ok(ToolOutput::new(object(json!({
        "name": name,
        "placeholders": placeholders,
    }))))
}

/// Every readable saved layout, read off the main thread. One the app can't
/// read is left out, as the in-app picker leaves it out.
async fn read_layouts(tools: &ToolContext) -> Result<Vec<SavedLayout>, ToolError> {
    let dir: PathBuf = tools.foreground.run(|cx| shell::layouts_dir(cx)).await?;
    let (layouts, _unreadable) = tokio::task::spawn_blocking(move || saved_layouts::load_all(&dir))
        .await
        .map_err(|error| {
            log::error!(target: "fernrohr::mcp", "reading saved layouts failed: {error}");
            ToolError::Internal
        })?;
    Ok(layouts)
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

#[cfg(test)]
pub(in crate::mcp) mod test_support;
#[cfg(test)]
mod tests;
