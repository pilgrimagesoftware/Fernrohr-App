//! Saving and restoring an object panel with the window's dock layout. One
//! pair of functions owns the saved shape, read both by the dock's restore and
//! by the window when it rebuilds its panel keys.

use super::panel::ObjectDetailPanel;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::ui::nav::{NavTarget, ObjectTarget};
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::dock::{PanelInfo, panel_handle, register_panel};
use gpui_kit::*;
use kube::core::GroupVersionKind;
use serde_json::{Value, json};

pub fn register_restore(cx: &mut App) {
    register_panel(cx, "ObjectDetail", |context, _window, cx| {
        let PanelInfo::Panel(state) = context.info() else {
            panic!("ObjectDetail layout state must be a panel");
        };
        let context_name = state["context_name"]
            .as_str()
            .expect("ObjectDetail layout state must name its cluster")
            .to_string();
        let target =
            target_from_state(state).expect("ObjectDetail layout state must name its object");
        let scope = PanelScope::new(NavTarget::Object(target.clone()), context_name);
        panel_handle(cx.new(|cx| ObjectDetailPanel::new(target, scope, cx)))
    });
}

/// What a panel over `target` in `context_name` saves.
pub(super) fn dump_target(target: &ObjectTarget, context_name: &str) -> Value {
    let gvk = &target.kind.gvk;
    json!({
        "context_name": context_name,
        "group": gvk.group,
        "version": gvk.version,
        "kind": gvk.kind,
        "plural": target.kind.plural,
        "namespaced": target.kind.namespaced,
        "object_namespace": target.namespace,
        "object_name": target.name,
    })
}

/// The object a saved panel was over, or `None` for state that doesn't name
/// one.
pub fn target_from_state(state: &Value) -> Option<ObjectTarget> {
    let kind = DiscoveredKind {
        gvk: GroupVersionKind::gvk(
            state["group"].as_str()?,
            state["version"].as_str()?,
            state["kind"].as_str()?,
        ),
        plural: state["plural"].as_str()?.to_string(),
        namespaced: state["namespaced"].as_bool()?,
    };
    Some(ObjectTarget {
        namespace: kind
            .namespaced
            .then(|| state["object_namespace"].as_str().map(str::to_string))
            .flatten(),
        kind,
        name: state["object_name"].as_str()?.to_string(),
    })
}
