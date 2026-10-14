//! Copy YAML's read: each checked object's manifest as the YAML view shows it -
//! a Secret's values redacted - without `managedFields`, which `kubectl get -o
//! yaml` hides too. Rows keep only what they show, so the objects are fetched.

use super::{BulkTarget, ObjectOutcome};
use crate::k8s::resource::resource_actions::{ActionFailure, api_for};

/// The manifests that could be read and written as YAML, in `targets`' order,
/// and the failure of each one that couldn't.
pub(crate) async fn manifests(
    client: kube::Client,
    targets: &[BulkTarget],
) -> (Vec<String>, Vec<ObjectOutcome>) {
    let mut manifests = Vec::new();
    let mut failures = Vec::new();
    for target in targets {
        let fetched = api_for(client.clone(), &target.kind, target.namespace.as_deref())
            .get(&target.name)
            .await;
        match fetched {
            Ok(mut object) => {
                crate::k8s::resource::object_detail::redact_secret_values(
                    &target.kind,
                    &mut object,
                );
                object.metadata.managed_fields = None;
                match serde_yaml_ng::to_string(&object) {
                    Ok(yaml) => manifests.push(yaml),
                    Err(error) => {
                        let message = format!("Couldn't write it as YAML: {error}");
                        failures.push(ObjectOutcome::of(
                            target,
                            Err(ActionFailure {
                                detail: message.clone(),
                                message,
                            }),
                        ));
                    }
                }
            }
            Err(error) => failures.push(ObjectOutcome::of(
                target,
                Err(ActionFailure::from_kube(&error)),
            )),
        }
    }
    (manifests, failures)
}
