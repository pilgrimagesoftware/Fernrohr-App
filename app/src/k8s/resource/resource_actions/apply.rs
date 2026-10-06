//! Editing a resource as YAML (`k9s-remaining-keybindings` section 2): the text
//! an edit starts from, what makes edited text a manifest this resource can
//! take, and saving it as a server-side apply.
//!
//! The apply is not forced: a field another manager owns, or a
//! `resourceVersion` the object has moved past, comes back as a conflict for
//! the user to resolve rather than being silently taken over.

use super::{ActionFailure, api_for};
use crate::k8s::cluster::discovery::DiscoveredKind;
use kube::api::{DynamicObject, Patch, PatchParams};

/// The field manager Fernrohr applies as.
pub(crate) const FIELD_MANAGER: &str = "fernrohr";

/// `object` as the YAML an edit starts from: without `metadata.managedFields`,
/// which an apply must not carry, and `status`, which the main resource
/// doesn't take. `resourceVersion` stays, so an edit of a stale copy conflicts.
pub(crate) fn edit_text(object: &DynamicObject) -> String {
    let mut object = object.clone();
    object.metadata.managed_fields = None;
    if let Some(data) = object.data.as_object_mut() {
        data.remove("status");
    }
    serde_yaml_ng::to_string(&object).unwrap_or_default()
}

/// `text` as a manifest for `name` of `kind` in `namespace`, or why it isn't
/// one: not YAML, not an object, or another kind or object than the one edited.
pub(crate) fn parse_manifest(
    text: &str,
    kind: &DiscoveredKind,
    name: &str,
    namespace: Option<&str>,
) -> Result<DynamicObject, String> {
    let mut object: DynamicObject =
        serde_yaml_ng::from_str(text).map_err(|error| format!("Not a valid manifest: {error}"))?;
    let api_version = kind.gvk.api_version();
    let types = object.types.clone().unwrap_or_default();
    if types.api_version != api_version || types.kind != kind.gvk.kind {
        return Err(format!(
            "This edits a {} ({api_version}); the manifest is a {} ({}).",
            kind.gvk.kind, types.kind, types.api_version
        ));
    }
    if object.metadata.name.as_deref() != Some(name) {
        return Err(format!(
            "This edits {name}; the manifest names {}.",
            object.metadata.name.as_deref().unwrap_or("nothing")
        ));
    }
    if kind.namespaced {
        match (object.metadata.namespace.as_deref(), namespace) {
            (None, Some(namespace)) => object.metadata.namespace = Some(namespace.to_string()),
            (Some(found), Some(expected)) if found != expected => {
                return Err(format!(
                    "This edits an object in {expected}; the manifest is in {found}."
                ));
            }
            _ => {}
        }
    }
    Ok(object)
}

/// Applies `object` as `kind` in `namespace`, returning the object the server
/// now holds.
pub(crate) async fn apply(
    client: kube::Client,
    kind: &DiscoveredKind,
    namespace: Option<&str>,
    object: DynamicObject,
) -> Result<DynamicObject, ActionFailure> {
    let name = object.metadata.name.clone().unwrap_or_default();
    api_for(client, kind, namespace)
        .patch(
            &name,
            &PatchParams::apply(FIELD_MANAGER),
            &Patch::Apply(&object),
        )
        .await
        .map_err(|error| ActionFailure::from_kube(&error))
}
