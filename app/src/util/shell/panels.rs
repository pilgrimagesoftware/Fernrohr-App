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
}

pub(super) fn restored_panel_keys(state: &PanelState) -> Vec<PanelKey> {
    let mut keys = state
        .children
        .iter()
        .flat_map(restored_panel_keys)
        .collect::<Vec<_>>();
    let PanelInfo::Panel(data) = &state.info else {
        return keys;
    };
    let context_name = data["context_name"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let namespaces = serde_json::from_value(data["namespaces"].clone()).unwrap_or_default();
    let target = match state.panel_name.as_str() {
        "Pods" => NavTarget::pods(),
        "Logs" => NavTarget::Logs,
        "PodDetail" => NavTarget::pod(
            data["pod_namespace"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            data["pod_name"].as_str().unwrap_or_default().to_string(),
        ),
        "ObjectDetail" => match crate::k8s::resource::object_detail::target_from_state(data) {
            Some(object) => NavTarget::Object(object),
            None => return keys,
        },
        "Resource" => NavTarget::Kind(crate::k8s::cluster::discovery::DiscoveredKind {
            gvk: GroupVersionKind::gvk(
                data["group"].as_str().unwrap_or_default(),
                data["version"].as_str().unwrap_or("v1"),
                data["kind"].as_str().unwrap_or("Resource"),
            ),
            plural: data["plural"].as_str().unwrap_or("resources").to_string(),
            namespaced: data["namespaced"].as_bool().unwrap_or(false),
        }),
        _ => return keys,
    };
    keys.push(PanelKey {
        target,
        context_name,
        namespaces,
    });
    keys
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
    if !matches!(target, NavTarget::Logs | NavTarget::Pod(_)) {
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
