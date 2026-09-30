//! Reading the object: one `get` through the discovered kind's `ApiResource`,
//! plus the events naming it, and the states the panel moves through.

use super::redact;
use crate::k8s::resource::events::{self, InvolvedObject};
use crate::ui::nav::ObjectTarget;
use k8s_openapi::api::core::v1::Event as K8sEvent;
use kube::Api;
use kube::api::{ApiResource, DynamicObject};

/// The events naming the object, or why they couldn't be listed - kept apart
/// from the object itself, as pod detail does, so a user who may read the
/// object but not list events still gets the object.
pub(super) type ObjectEvents = Result<Vec<K8sEvent>, String>;

pub(super) enum ObjectDetailState {
    Loading,
    Loaded(Box<DynamicObject>, ObjectEvents),
    /// The object doesn't exist - deleted, or never created (a Secret mounted
    /// `optional`). A state, not an error: following a link to it is normal.
    NotFound,
    /// `message` is readable prose; `detail` the full technical rendering.
    Failed {
        message: String,
        detail: String,
    },
}

pub(super) enum ObjectFetch {
    Found(Box<DynamicObject>, ObjectEvents),
    NotFound,
}

/// Fetches `target` and the events naming it. The object is redacted before
/// it is returned (see [`redact`]), so nothing downstream - the structured
/// view, the YAML, a test - ever holds a Secret's values.
pub(super) async fn fetch_object(
    client: kube::Client,
    target: ObjectTarget,
) -> Result<ObjectFetch, (String, String)> {
    let resource = ApiResource::from_gvk_with_plural(&target.kind.gvk, &target.kind.plural);
    let api: Api<DynamicObject> = match (&target.namespace, target.kind.namespaced) {
        (Some(namespace), true) => Api::namespaced_with(client.clone(), namespace, &resource),
        _ => Api::all_with(client.clone(), &resource),
    };
    let mut object = match api.get(&target.name).await {
        Ok(object) => object,
        Err(kube::Error::Api(status)) if status.code == 404 => return Ok(ObjectFetch::NotFound),
        Err(error) => {
            return Err((
                crate::k8s::error::describe(&error),
                crate::k8s::error::detail(&error),
            ));
        }
    };
    redact::redact(&target.kind, &mut object);
    let events = events::list(
        client,
        &InvolvedObject {
            kind: &target.kind.gvk.kind,
            namespace: target.namespace.as_deref(),
            name: &target.name,
            uid: object.metadata.uid.as_deref(),
        },
    )
    .await;
    Ok(ObjectFetch::Found(Box::new(object), events))
}
