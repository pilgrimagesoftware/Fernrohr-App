//! A CronJob's two actions:
//!
//! - Triggering it: a Job built from its `jobTemplate`, as `kubectl create job
//!   --from=cronjob/<name>` builds one - the template's labels, annotations
//!   and spec, the `cronjob.kubernetes.io/instantiate: manual` annotation, and
//!   the CronJob as its controlling owner, so it shows in the CronJob's
//!   history and goes when the CronJob does. [`plan_trigger`] builds it (and
//!   its name) so a caller can show it, [`trigger`] creates it.
//! - Suspending or resuming its schedule: `spec.suspend`, alone.

use super::{ActionError, api_for, patch_params};
use crate::k8s::cluster::discovery::DiscoveredKind;
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
use kube::Api;
use kube::api::{Patch, PostParams};
use serde_json::json;

/// The annotation kubectl marks a hand-made Job with.
const INSTANTIATE_ANNOTATION: &str = "cronjob.kubernetes.io/instantiate";
/// A Job's name becomes its pods' `job-name` label, whose values stop at 63.
const MAX_JOB_NAME: usize = 63;

/// The Job [`trigger`] will create.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TriggerPlan {
    pub(crate) job_name: String,
    job: Job,
}

/// Builds the Job for CronJob `name`, named `<name>-manual-<suffix>`.
pub(crate) async fn plan_trigger(
    client: kube::Client,
    namespace: &str,
    name: &str,
    suffix: &str,
) -> Result<TriggerPlan, ActionError> {
    let cronjob = Api::<CronJob>::namespaced(client, namespace)
        .get(name)
        .await?;
    job_from(&cronjob, suffix)
}

/// Creates `plan`'s Job.
pub(crate) async fn trigger(
    client: kube::Client,
    namespace: &str,
    plan: &TriggerPlan,
) -> Result<(), ActionError> {
    Api::<Job>::namespaced(client, namespace)
        .create(&PostParams::default(), &plan.job)
        .await?;
    Ok(())
}

pub(super) fn job_from(cronjob: &CronJob, suffix: &str) -> Result<TriggerPlan, ActionError> {
    let name = cronjob.metadata.name.clone().unwrap_or_default();
    let template = &cronjob.spec.job_template;
    if template.spec.is_none() {
        return Err(ActionError::Precondition(format!(
            "CronJob {name:?} has no job template"
        )));
    }
    let job_name = manual_job_name(&name, suffix);
    let template_meta = template.metadata.clone().unwrap_or_default();
    let mut annotations = template_meta.annotations.unwrap_or_default();
    annotations.insert(INSTANTIATE_ANNOTATION.to_string(), "manual".to_string());
    let job = Job {
        metadata: ObjectMeta {
            name: Some(job_name.clone()),
            namespace: cronjob.metadata.namespace.clone(),
            labels: template_meta.labels,
            annotations: Some(annotations),
            owner_references: Some(vec![OwnerReference {
                api_version: "batch/v1".to_string(),
                kind: "CronJob".to_string(),
                name,
                uid: cronjob.metadata.uid.clone().unwrap_or_default(),
                controller: Some(true),
                block_owner_deletion: Some(true),
            }]),
            ..ObjectMeta::default()
        },
        spec: template.spec.clone(),
        status: None,
    };
    Ok(TriggerPlan { job_name, job })
}

/// `<cronjob>-manual-<suffix>`, the CronJob's name cut short when the whole
/// would pass [`MAX_JOB_NAME`].
pub(super) fn manual_job_name(cronjob: &str, suffix: &str) -> String {
    let tail = format!("-manual-{suffix}");
    let room = MAX_JOB_NAME.saturating_sub(tail.len());
    let head = cronjob[..cronjob.len().min(room)].trim_end_matches(['-', '.']);
    format!("{head}{tail}")
}

/// Whether CronJob `name`'s schedule is suspended now.
pub(crate) async fn cronjob_suspended(
    client: kube::Client,
    kind: &DiscoveredKind,
    namespace: &str,
    name: &str,
) -> Result<bool, ActionError> {
    let cronjob = api_for(client, kind, Some(namespace)).get(name).await?;
    Ok(cronjob.data["spec"]["suspend"].as_bool().unwrap_or(false))
}

/// Suspends or resumes CronJob `name`'s schedule.
pub(crate) async fn set_cronjob_suspended(
    client: kube::Client,
    kind: &DiscoveredKind,
    namespace: &str,
    name: &str,
    suspended: bool,
) -> Result<(), ActionError> {
    let patch = json!({ "spec": { "suspend": suspended } });
    api_for(client, kind, Some(namespace))
        .patch(name, &patch_params(), &Patch::Merge(&patch))
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manual_job_name_fits_a_label_value() {
        assert_eq!(manual_job_name("backup", "a1b2c"), "backup-manual-a1b2c");
        let long = "n".repeat(80);
        let name = manual_job_name(&long, "a1b2c");
        assert_eq!(name.len(), MAX_JOB_NAME);
        assert!(name.ends_with("-manual-a1b2c"));
        // A cut that lands on a separator doesn't leave it dangling.
        let dashed = format!("{}-x", "n".repeat(49));
        assert!(!manual_job_name(&dashed, "a1b2c").contains("--"));
    }
}
