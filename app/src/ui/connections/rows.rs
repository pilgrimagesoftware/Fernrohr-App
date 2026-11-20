//! What the Connections window lists, as plain data read from the registry: one
//! row per context with at least one holder, its state as the status bar shows
//! it, its bound tunnel, and who holds it. No rendering and no I/O (the tunnel
//! names are cached by the window), so tests assert on rows directly.
//!
//! Lingering holds (an ended agent session's, kept open briefly) are not listed
//! as holders: no agent owns them. A context held only by them still gets a row,
//! because the connection is open and Disconnect can close it; the row says so
//! ([`ConnectionRow::lingering`]).

use crate::k8s::cluster::session::{AgentId, ClusterRegistry, Holder, LiveAgents};
use crate::ui::picker_tunnel::TunnelChoice;
use crate::ui::status_bar::{StatusBarView, StatusItem};
use gpui_kit::App;
use std::collections::BTreeMap;
use std::time::Instant;

/// One holder of a row's context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum HolderLabel {
    /// A window, by its title.
    Window { title: String },
    /// An MCP session. `name` is what its client reported about itself: a label
    /// for the user, never an identity and never used to decide anything.
    Agent {
        id: AgentId,
        name: Option<String>,
        /// Whether this agent's request opened the connection.
        connected_it: bool,
    },
}

impl HolderLabel {
    /// How the row names the holder: a window by its title, an agent as
    /// self-reported, or as unnamed.
    pub(super) fn text(&self) -> String {
        match self {
            HolderLabel::Window { title } => title.clone(),
            HolderLabel::Agent {
                name: Some(name), ..
            } => format!("reports itself as \u{201c}{name}\u{201d}"),
            HolderLabel::Agent { name: None, .. } => "Unnamed agent".to_string(),
        }
    }
}

/// One listed connection.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ConnectionRow {
    pub(super) context: String,
    /// The state, with the status bar's icon and text.
    pub(super) item: StatusItem,
    pub(super) tunnel: Option<String>,
    /// Windows first, then agents.
    pub(super) holders: Vec<HolderLabel>,
    /// Whether an ended agent session's hold is still keeping it open.
    pub(super) lingering: bool,
}

impl ConnectionRow {
    /// Each agent holding this context with how the row names it, in row order.
    pub(super) fn agent_labels(&self) -> Vec<(AgentId, String)> {
        self.holders
            .iter()
            .filter_map(|holder| match holder {
                HolderLabel::Agent { id, .. } => Some((*id, holder.text())),
                HolderLabel::Window { .. } => None,
            })
            .collect()
    }

    /// The titles of the windows holding this context.
    pub(super) fn window_titles(&self) -> Vec<String> {
        self.holders
            .iter()
            .filter_map(|holder| match holder {
                HolderLabel::Window { title } => Some(title.clone()),
                HolderLabel::Agent { .. } => None,
            })
            .collect()
    }
}

/// The tunnel names the window caches, refreshed only when `tunnels.toml`
/// changes - the same rule the status bar follows.
#[derive(Default)]
pub(super) struct TunnelNames {
    pub(super) choices: Vec<TunnelChoice>,
    pub(super) bindings: BTreeMap<String, String>,
}

impl TunnelNames {
    fn of(&self, context: &str) -> Option<String> {
        let id = self.bindings.get(context)?;
        self.choices
            .iter()
            .find(|choice| &choice.id == id)
            .map(|choice| choice.name.clone())
    }
}

/// Every context with an open connection and a holder, by name.
pub(super) fn rows(cx: &App, tunnels: &TunnelNames) -> Vec<ConnectionRow> {
    let now = Instant::now();
    let mut contexts = ClusterRegistry::open_contexts(cx);
    contexts.sort();
    contexts
        .into_iter()
        .filter_map(|context| {
            let held = ClusterRegistry::holders(cx, &context);
            if held.is_empty() {
                return None;
            }
            let connected_by = ClusterRegistry::connected_by(cx, &context);
            let windows = crate::util::shell::windows_holding(&context, cx);
            let lingering = held.iter().any(|h| matches!(h, Holder::Lingering(_)));
            // The registry's holders are unordered: windows as they stack (the
            // frontmost first), then agents as they connected, so a row reads
            // the same on every draw.
            let mut held = held;
            held.sort_by_key(|holder| match holder {
                Holder::Window(id) => (
                    0,
                    windows
                        .iter()
                        .position(|(handle, _)| handle.window_id() == *id)
                        .unwrap_or(usize::MAX),
                    None,
                ),
                Holder::Agent(id) => (1, 0, Some(*id)),
                Holder::Lingering(_) => (2, 0, None),
            });
            let holders = held
                .iter()
                .filter_map(|holder| match holder {
                    Holder::Window(id) => Some(HolderLabel::Window {
                        title: windows
                            .iter()
                            .find(|(handle, _)| handle.window_id() == *id)
                            .map_or_else(|| "A window".to_string(), |(_, title)| title.clone()),
                    }),
                    Holder::Agent(id) => Some(HolderLabel::Agent {
                        id: *id,
                        name: LiveAgents::name(cx, *id),
                        connected_it: connected_by == Some(*holder),
                    }),
                    Holder::Lingering(_) => None,
                })
                .collect();
            Some(ConnectionRow {
                item: StatusBarView::item_for(&context, cx, now),
                tunnel: tunnels.of(&context),
                context,
                holders,
                lingering,
            })
        })
        .collect()
}
