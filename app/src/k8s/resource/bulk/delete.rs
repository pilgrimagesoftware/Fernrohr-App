//! Bulk Delete's request: the single-row delete (`resource_actions`), with the
//! listed object's UID as a precondition, so an object deleted and recreated
//! under the same name since the list is left alone. A 404 is reported as
//! already gone, not as deleted.

use super::{BulkTarget, Done};
use crate::k8s::resource::resource_actions::{ActionFailure, api_for, delete_params};
use kube::api::{DeleteParams, Preconditions};

/// The delete options: a normal delete, only of the object with `uid`.
pub(super) fn params(uid: Option<&str>) -> DeleteParams {
    DeleteParams {
        preconditions: uid.map(|uid| Preconditions {
            uid: Some(uid.to_string()),
            resource_version: None,
        }),
        ..delete_params(false)
    }
}

/// Deletes `target`, if it is still the object the row listed.
pub(super) async fn delete(
    client: kube::Client,
    target: &BulkTarget,
) -> Result<Done, ActionFailure> {
    let api = api_for(client, &target.kind, target.namespace.as_deref());
    match api
        .delete(&target.name, &params(target.uid.as_deref()))
        .await
    {
        Ok(_) => Ok(Done::Changed),
        Err(kube::Error::Api(status)) if status.code == 404 => Ok(Done::AlreadyGone),
        // The UID precondition failed: the name is now another object's.
        Err(error @ kube::Error::Api(_)) if is_conflict(&error) => Err(ActionFailure {
            message: "Not deleted: an object of the same name was created since it was listed."
                .to_string(),
            detail: ActionFailure::from_kube(&error).detail,
        }),
        Err(error) => Err(ActionFailure::from_kube(&error)),
    }
}

fn is_conflict(error: &kube::Error) -> bool {
    matches!(error, kube::Error::Api(status) if status.code == 409)
}
