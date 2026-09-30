//! Reading the pod: one `get` plus the events naming it, and the states the
//! panel moves through while and after that happens.

use super::format::non_empty;
use crate::k8s::resource::events::{self, InvolvedObject};
use k8s_openapi::api::core::v1::Event as K8sEvent;
use k8s_openapi::api::core::v1::Pod;
use kube::Api;

/// The events naming a pod, or why they could not be listed. Kept apart from
/// the pod's own fetch result: a user allowed to `get` pods but not to `list`
/// events still gets the pod, and the Events tab says why it is empty rather
/// than claiming there were none.
pub(super) type PodEvents = Result<Vec<K8sEvent>, String>;

/// What the panel knows about the pod it is scoped to.
pub(super) enum PodDetailState {
    Loading,
    Loaded(Box<Pod>, PodEvents),
    /// The pod is gone. Its own state rather than an error: a detail panel that
    /// outlives its pod is a normal thing to have left open, not a failure.
    NotFound,
    /// `message` is what the panel shows by default - readable prose, not a
    /// client library's `Debug` dump (`1-window-context-bar` bug 2); `detail`
    /// is that same failure's full technical rendering, kept alongside rather
    /// than discarded.
    Failed {
        message: String,
        detail: String,
    },
}

/// One fetch's outcome, so a 404 is told apart from every other error before it
/// reaches the panel's state.
pub(super) enum PodFetch {
    Found(Box<Pod>, PodEvents),
    NotFound,
}

/// Fetches the pod and, alongside it, the events naming it - a single round
/// trip's worth of state rather than a second fetch lifecycle to manage, since
/// the Events tab has nothing to show until the pod itself has loaded anyway.
/// A 404 on the pod skips the events lookup entirely: there is nothing left to
/// name events by. Any other failure is `(message, detail)` - see
/// `PodDetailState::Failed`'s doc comment.
pub(super) async fn fetch_pod(
    client: kube::Client,
    namespace: String,
    name: String,
) -> Result<PodFetch, (String, String)> {
    let api: Api<Pod> = Api::namespaced(client.clone(), &namespace);
    let pod = match api.get(&name).await {
        Ok(pod) => pod,
        Err(kube::Error::Api(status)) if status.code == 404 => return Ok(PodFetch::NotFound),
        Err(error) => {
            return Err((
                crate::k8s::error::describe(&error),
                crate::k8s::error::detail(&error),
            ));
        }
    };
    let events = events::list(
        client,
        &InvolvedObject {
            kind: "Pod",
            namespace: Some(&namespace),
            name: &name,
            uid: non_empty(&pod.metadata.uid),
        },
    )
    .await;
    Ok(PodFetch::Found(Box::new(pod), events))
}
