//! How the window recognises a panel it already holds: `PanelKey`, the open-panel bookkeeping, restored panel keys, and which context a pod-scoped panel reads.

use super::*;

/// Enough to recognise a panel the dock already holds: which kind, in which
/// cluster, over which namespace scope. The spec's rule is "the same kind,
/// cluster, and namespace scope" (9.3), so all three are in the key - a window
/// with two clusters open, or two namespace pickers on one kind, needs them.
///
/// `namespace` is `None` for a panel whose title-bar picker still reads "All
/// namespaces", which is every panel until the picker has a narrower scope to
/// offer.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(super) struct PanelKey {
    pub(super) target: NavTarget,
    pub(super) context_name: String,
    pub(super) namespaces: Vec<String>,
}

impl From<&PanelScope> for PanelKey {
    /// Derived rather than built alongside, so the key a panel is filed under
    /// and the scope its title bar draws cannot describe different panels -
    /// which is exactly the disagreement 10.2's namespace picker would
    /// otherwise create.
    fn from(scope: &PanelScope) -> Self {
        Self {
            target: scope.target.clone(),
            context_name: scope.context_name.clone(),
            namespaces: scope.namespaces.clone(),
        }
    }
}

/// A panel the window opened, and how to find it again in the dock.
pub(super) struct OpenPanel {
    pub(super) key: PanelKey,
    pub(super) id: PanelId,
    /// The typed handle, when this window built the panel. `None` for one
    /// restored by the dock from layout - that one exists only as a
    /// `PanelId`, with no entity the window ever held.
    ///
    /// Kept so a request that arrives for an already-open panel can do more
    /// than focus it: `y` on a pod whose detail panel is already showing
    /// fields has to switch that panel to the YAML, and switching needs the
    /// entity, not the id.
    pub(super) panel: Option<nav::OpenedPanel>,
    /// The tab group the panel was last seen in, refreshed on every layout
    /// change. Once the panel is closed the dock no longer says where it was,
    /// so this is how focus finds the tab that took its place.
    pub(super) group: Option<gpui_kit::component::dock::NodeId>,
    /// Notes this panel as the window's last-focused dock panel whenever focus
    /// enters it ([`MainWindow::watch_panel_focus`]); dropped with the entry.
    pub(super) _focus_watch: Option<gpui_kit::Subscription>,
}

/// One key per panel a saved layout restores, in the order the dock builds
/// them - `None` for a panel the window can't key (an unrecognised kind, or
/// state it can't read, which restores as `ui::unrestored`'s placeholder).
///
/// One entry per built panel, keyed or not, is what lets the window pair these
/// with the dock's panel ids by position: skipping a panel would shift every
/// key after it onto the wrong panel. Delegates its walk to
/// [`restored_panel_leaves`] - same order, minus the raw state section 5's
/// `saved_layouts::load` also needs.
pub(super) fn restored_panel_keys(state: &PanelState) -> Vec<Option<PanelKey>> {
    restored_panel_leaves(state)
        .into_iter()
        .map(|(_, key)| key)
        .collect()
}

/// [`restored_panel_keys`]'s own pairs, each with the leaf [`PanelState`] its
/// key (or lack of one) came from - `saved_layouts::load`'s `load_add` (section
/// 5.1) needs the raw state too, to wrap a panel scoped to a context this
/// window doesn't hold as `ui::unrestored`'s placeholder, keeping its
/// original content rather than only deciding whether to open it.
///
/// The walk mirrors gpui-base's `PaneTree::from_state` - stacks recurse, tab
/// groups flatten nested tabs and skip empty `TabPanel` leaves, and every
/// other node is one panel.
pub(super) fn restored_panel_leaves(state: &PanelState) -> Vec<(PanelState, Option<PanelKey>)> {
    match &state.info {
        PanelInfo::Stack { .. } => state
            .children
            .iter()
            .flat_map(restored_panel_leaves)
            .collect(),
        PanelInfo::Tabs { .. } => tab_panel_leaves(&state.children),
        PanelInfo::Panel(_) if state.panel_name == TAB_PANEL_NAME => Vec::new(),
        PanelInfo::Panel(_) => vec![(state.clone(), panel_key(state))],
    }
}

/// The name a tab group saves under; a leaf carrying it is an empty group.
const TAB_PANEL_NAME: &str = "TabPanel";

fn tab_panel_leaves(children: &[PanelState]) -> Vec<(PanelState, Option<PanelKey>)> {
    children
        .iter()
        .flat_map(|child| match &child.info {
            PanelInfo::Tabs { .. } => tab_panel_leaves(&child.children),
            PanelInfo::Panel(_) if child.panel_name == TAB_PANEL_NAME => Vec::new(),
            _ => vec![(child.clone(), panel_key(child))],
        })
        .collect()
}

/// The key one saved panel restores under, or `None` when it isn't a panel
/// the window knows or its state doesn't say what it shows.
fn panel_key(state: &PanelState) -> Option<PanelKey> {
    let PanelInfo::Panel(data) = &state.info else {
        return None;
    };
    let context_name = data["context_name"].as_str()?.to_string();
    let namespaces = serde_json::from_value(data["namespaces"].clone()).unwrap_or_default();
    let target = match state.panel_name.as_str() {
        "Pods" => NavTarget::pods(),
        "Logs" if data.get("label_logs").is_some() => {
            NavTarget::LabelLogs(crate::util::logs::labels_from_state(data)?.0)
        }
        "Logs" => match crate::util::logs::pinned_from_state(data, &context_name) {
            Some(pinned) => NavTarget::PodLogs(crate::ui::nav::PodRef {
                namespace: pinned.namespace,
                name: pinned.name,
            }),
            None => NavTarget::Logs,
        },
        "Exec" => NavTarget::Exec(crate::k8s::resource::exec::ExecTarget {
            namespace: data["namespace"].as_str()?.to_string(),
            pod: data["pod"].as_str()?.to_string(),
            container: data["container"].as_str()?.to_string(),
        }),
        "PodDetail" => NavTarget::pod(
            data["pod_namespace"].as_str()?.to_string(),
            data["pod_name"].as_str()?.to_string(),
        ),
        "ObjectDetail" => NavTarget::Object(
            crate::k8s::resource::object_detail::target_from_state(data)?,
        ),
        // A list panel, and the placeholder that stands in for one: both save the
        // kind the same way, and both are a `Kind` target.
        "ObjectList" | "Resource" => {
            NavTarget::Kind(crate::k8s::resource::object_list::restore::from_state(data)?.kind)
        }
        "Events" => {
            crate::k8s::resource::events_browser::restore::from_state(data)?;
            NavTarget::Kind(crate::k8s::cluster::discovery::DiscoveredKind::events())
        }
        _ => return None,
    };
    Some(PanelKey {
        target,
        context_name,
        namespaces,
    })
}

/// The cluster context a Logs or Pod-detail panel should scope itself to: the
/// context that published the currently selected pod ([`SelectedPod`]), when
/// `contexts` - this window's own - includes it; [`contexts[active]`](usize)
/// while nothing is selected yet (a bare `nav.show_logs` before any pod has
/// been clicked, or a restored panel with no live selection at all). Every
/// other target is not pod-scoped and always reads `contexts[active]`.
///
/// A selection published by a context this window does not hold is refused
/// rather than opened against `active` instead - that silent substitution
/// (`1-window-context-bar` bug 1) is what streamed a pod selected in one
/// context's Pods panel against a *different* context, turning a real pod
/// into a 404. [`MainWindow::open_target_with_view`] no-ops on `None`, after
/// this has logged why.
pub(super) fn pod_scoped_context(
    target: &NavTarget,
    contexts: &[String],
    active: usize,
    cx: &App,
) -> Option<String> {
    if !matches!(
        target,
        NavTarget::Logs | NavTarget::PodLogs(_) | NavTarget::Pod(_)
    ) {
        return Some(contexts[active].clone());
    }
    match cx
        .try_global::<SelectedPod>()
        .and_then(|selected| selected.0.as_ref())
    {
        None => Some(contexts[active].clone()),
        Some(selection) if contexts.iter().any(|held| held == &selection.context_name) => {
            Some(selection.context_name.clone())
        }
        Some(selection) => {
            log::warn!(
                "selected pod {}/{} belongs to context {:?}, which this window does not hold \
                 ({contexts:?}); not opening {target:?}",
                selection.namespace,
                selection.name,
                selection.context_name,
            );
            None
        }
    }
}

#[cfg(test)]
mod tests;
