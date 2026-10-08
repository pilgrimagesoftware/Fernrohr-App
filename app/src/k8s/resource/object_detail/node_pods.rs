//! #186: a Node's detail panel grows one region below its own sections - a
//! live, all-namespaces table of the pods scheduled on it (`spec.nodeName ==
//! node`), built once for `("", "Node")` and `None` for every other kind.
//!
//! The table itself is not duplicated: it is the standalone Pods panel's own
//! (`crate::k8s::resource::pods::PodsPanel`), scoped to this node
//! (`PodsPanel::scoped_to_node`) and embedded as a plain child view rather
//! than opened as a second dock panel - it shares the context's already-
//! shared pods watch and connection (`ClusterRegistry::subscribe_pods`), so a
//! Node's pods region and an open Pods list cost nothing extra, and its
//! Connecting/Failed states and row actions (`d`, `l`, `/`) all come for
//! free. It is never opened through `open_target`, so it is never dumped or
//! restored, and draws no title or dock chrome of its own - see `render`'s
//! header suppression for the one piece of `PodsPanel` this embedding asks it
//! to leave out.
//!
//! The one gap embedding doesn't close: the standalone panel leaves bare
//! `enter` to its quick look, so this module adds `OpenNodePod` for it,
//! scoped to [`KEY_CONTEXT`] alone - the region, not the rest of the detail
//! panel.

use super::panel::ObjectDetailPanel;
use crate::command::{Command, CommandRegistry};
use crate::k8s::resource::pods::{PodsPanel, SelectedPod};
use crate::ui::nav::{NavTarget, ObjectTarget, ShowPodDetail};
use crate::ui::panel_title::PanelScope;
use gpui_kit::*;

actions!(object_detail, [OpenNodePod]);

/// Added to the embedded table's own wrapping `div`, not the whole detail
/// panel, so `enter` opens a pod only while focus is in this region.
pub(super) const KEY_CONTEXT: &str = "NodePodsTable";
pub(super) const OPEN_KEY: &str = "enter";
const OPEN_COMMAND_ID: &str = "object_detail.node_pods.open_detail";

/// Whether `target` is this region's one kind: the core, cluster-scoped
/// `Node` - the same `(group, kind)` pair `sections::sections_for` dispatches
/// `cluster::node` on, so the two can't drift about what counts as a Node.
fn applies_to(target: &ObjectTarget) -> bool {
    target.kind.gvk.group.is_empty() && target.kind.gvk.kind == "Node"
}

/// The embedded, node-scoped Pods table for `target` in `context_name` -
/// `None` for every kind but Node. Built once, with the panel: a Node's
/// identity (so its name) is fixed for the life of the panel that shows it,
/// the same way `target` itself is.
pub(super) fn build(
    target: &ObjectTarget,
    context_name: &str,
    cx: &mut Context<ObjectDetailPanel>,
) -> Option<Entity<PodsPanel>> {
    if !applies_to(target) {
        return None;
    }
    let scope = PanelScope::new(NavTarget::pods(), context_name.to_string());
    let node = target.name.clone();
    Some(cx.new(|cx| PodsPanel::new(scope, cx).scoped_to_node(node)))
}

/// `object_detail.node_pods.open_detail`: a palette entry and keymap override
/// while the region has focus, bound to `enter` by default.
pub(super) fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: OPEN_COMMAND_ID,
        title: "Node Pods: Open Selected Pod's Detail",
        default_binding: OPEN_KEY,
        context: Some("NodePodsTable && !Input"),
        action: Box::new(OpenNodePod),
        menu: None,
    });
}

impl ObjectDetailPanel {
    /// `OpenNodePod` (`enter`): the same open `d` already reaches through the
    /// embedded panel's own `DescribePod` handler, via the same app-scoped
    /// `SelectedPod` selection and `ShowPodDetail` window action every pod
    /// list opens a detail through.
    pub(super) fn on_action_open_node_pod(
        &mut self,
        _: &OpenNodePod,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = cx
            .try_global::<SelectedPod>()
            .is_some_and(|selected| selected.0.is_some());
        if selected {
            window.dispatch_action(Box::new(ShowPodDetail), cx);
        }
    }
}

#[cfg(test)]
mod tests;
