//! Turning an `open_panel` request into the [`NavTarget`] and namespace scope
//! the window opens. Pure: the kind is already resolved (`kinds::resolve`)
//! and the names already parsed (`names`), so this only decides whether the
//! scope fits the kind, and a request it refuses never reaches a window.

use super::super::error::ToolError;
use super::super::names::{LabelName, ObjectName};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::ui::nav::{NavTarget, ObjectTarget};

/// What to open for `kind`: its list when `name` is `None`, scoped to
/// `namespace` (or the context's default scope when that is `None` too), or
/// the named object's own detail panel - the panel a double-click on its row
/// opens.
pub(super) fn panel_target(
    kind: &DiscoveredKind,
    namespace: Option<LabelName>,
    name: Option<ObjectName>,
) -> Result<(NavTarget, Vec<String>), ToolError> {
    let namespace = namespace.map(|namespace| namespace.as_str().to_string());
    if namespace.is_some() && !kind.namespaced {
        return Err(invalid(format!(
            "{} is cluster-scoped; omit `namespace`",
            kind.gvk.kind
        )));
    }
    let Some(name) = name else {
        return Ok((
            NavTarget::Kind(kind.clone()),
            namespace.into_iter().collect(),
        ));
    };
    let name = name.as_str().to_string();
    let target = match (kind.namespaced, namespace) {
        (true, None) => {
            return Err(invalid(format!(
                "{} is namespaced; naming one needs its `namespace`",
                kind.gvk.kind
            )));
        }
        (true, Some(namespace)) if kind.is_core_pod() => NavTarget::pod(namespace, name),
        (_, namespace) => NavTarget::Object(ObjectTarget {
            kind: kind.clone(),
            namespace,
            name,
        }),
    };
    // A detail panel is keyed by its object, not by a list scope.
    Ok((target, Vec::new()))
}

fn invalid(message: String) -> ToolError {
    ToolError::InvalidArguments { message }
}

#[cfg(test)]
mod tests;
