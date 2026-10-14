//! Acting on several checked objects at once (`bulk-select-list-actions`): which
//! actions a checked set is offered ([`eligibility`]), how it is named in a
//! confirmation ([`summary`]), and the cluster calls themselves, run per object by
//! [`run`] so one object's failure is reported beside the others' successes.
//!
//! Each call is the one `kubectl` makes for the same operation. Restart,
//! rollback and scale are [`resource_actions`]'s, the same calls the agent
//! tools make. Delete ([`delete`]) is the single-row delete's request with the
//! listed object's UID as a precondition; cordon, drain ([`nodes`]) and label
//! or annotate ([`metadata`]) are this module's own.

use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::resource_actions::{self, ActionError, ActionFailure};
use futures_util::StreamExt as _;

mod delete;
mod eligibility;
mod manifests;
mod metadata;
mod nodes;
mod summary;

pub(crate) use eligibility::BulkAction;
pub(crate) use manifests::manifests;
pub(crate) use metadata::MetadataMap;
pub(crate) use summary::count_of_kinds;

/// One checked object.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BulkTarget {
    pub(crate) kind: DiscoveredKind,
    pub(crate) namespace: Option<String>,
    pub(crate) name: String,
    /// The UID of the object the row listed, when known: a delete is sent only
    /// if the object of that name is still this one.
    pub(crate) uid: Option<String>,
}

impl BulkTarget {
    /// `namespace/name`, or the name alone for a cluster-scoped object.
    pub(crate) fn display_name(&self) -> String {
        match &self.namespace {
            Some(namespace) => format!("{namespace}/{}", self.name),
            None => self.name.clone(),
        }
    }

    fn namespace(&self) -> &str {
        self.namespace.as_deref().unwrap_or_default()
    }
}

/// A bulk action that changes the cluster, with what it was asked to set.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Mutation {
    Delete,
    /// Stamped with the time the user confirmed, the same for every object.
    RestartRollout {
        at: jiff::Timestamp,
    },
    RollbackRollout,
    Scale {
        replicas: i32,
    },
    Cordon,
    Uncordon,
    /// Cordons, then evicts, waiting up to `eviction_timeout` for each pod
    /// ([`crate::consts::BULK_EVICTION_TIMEOUT`]).
    Drain {
        eviction_timeout: std::time::Duration,
    },
    /// Sets `key` to `value` in `map`, or - `value` is `None` - removes it.
    SetMetadata {
        map: MetadataMap,
        key: String,
        value: Option<String>,
    },
}

/// What a successful request did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Done {
    /// It made the change.
    Changed,
    /// There was nothing to change: the object was already gone.
    AlreadyGone,
}

/// How one checked object - or, for a drain, one of a node's pods - came out.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ObjectOutcome {
    pub(crate) target: BulkTarget,
    /// What within the target this outcome is about: a drained node's pod
    /// (`evict shop/web-1`). `None` for the target itself.
    pub(crate) part: Option<String>,
    pub(crate) result: Result<Done, ActionFailure>,
}

impl ObjectOutcome {
    /// `target`'s outcome: changed, or why not.
    fn of(target: &BulkTarget, result: Result<(), ActionFailure>) -> Self {
        Self::done(target, result.map(|()| Done::Changed))
    }

    fn done(target: &BulkTarget, result: Result<Done, ActionFailure>) -> Self {
        Self {
            target: target.clone(),
            part: None,
            result,
        }
    }
}

/// Runs `mutation` on every target, a few at a time, and returns every
/// object's outcome in `targets`' order - a failure never stops the rest.
pub(crate) async fn run(
    client: kube::Client,
    mutation: &Mutation,
    targets: &[BulkTarget],
) -> Vec<ObjectOutcome> {
    // Each future owns what it sends, so the stream's futures borrow nothing
    // from its closure.
    futures_util::stream::iter(targets.iter().cloned())
        .map(|target| {
            let (client, mutation) = (client.clone(), mutation.clone());
            async move { run_one(client, &mutation, &target).await }
        })
        .buffered(crate::consts::BULK_CONCURRENCY)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect()
}

/// One target's outcomes: one, or for a drain the node's and each failed pod's.
async fn run_one(
    client: kube::Client,
    mutation: &Mutation,
    target: &BulkTarget,
) -> Vec<ObjectOutcome> {
    let kind = &target.kind;
    let name = target.name.as_str();
    let result = match mutation {
        Mutation::Delete => {
            return vec![ObjectOutcome::done(
                target,
                delete::delete(client, target).await,
            )];
        }
        Mutation::RestartRollout { at } => {
            resource_actions::restart(client, kind, target.namespace(), name, *at)
                .await
                .map_err(failure)
        }
        Mutation::RollbackRollout => rollback(client, target).await.map_err(failure),
        Mutation::Scale { replicas } => {
            resource_actions::scale(client, kind, target.namespace(), name, *replicas)
                .await
                .map(|_| ())
                .map_err(failure)
        }
        Mutation::Cordon => nodes::set_unschedulable(client, kind, name, true).await,
        Mutation::Uncordon => nodes::set_unschedulable(client, kind, name, false).await,
        Mutation::Drain { eviction_timeout } => {
            return nodes::drain(client, target, *eviction_timeout).await;
        }
        Mutation::SetMetadata { map, key, value } => {
            metadata::set(client, target, *map, key, value.as_deref()).await
        }
    };
    vec![ObjectOutcome::of(target, result)]
}

/// `kubectl rollout undo`: back to the revision before the current one.
async fn rollback(client: kube::Client, target: &BulkTarget) -> Result<(), ActionError> {
    let (kind, namespace, name) = (&target.kind, target.namespace(), &target.name);
    let plan = resource_actions::plan_rollback(client.clone(), kind, namespace, name).await?;
    resource_actions::rollback(client, kind, namespace, name, &plan).await
}

/// An [`ActionError`] as the outcome report shows it.
pub(crate) fn failure(error: ActionError) -> ActionFailure {
    match error {
        ActionError::Kube(error) => ActionFailure::from_kube(&error),
        ActionError::Precondition(message) => ActionFailure {
            detail: message.clone(),
            message,
        },
    }
}

#[cfg(test)]
mod tests;
