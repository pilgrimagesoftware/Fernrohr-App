//! Reading the pod: one `get`, and the states the panel moves through while and
//! after that happens. Its events come from a live watch the panel runs once the
//! pod is in (`live_events`), not from this fetch.

use k8s_openapi::api::core::v1::Pod;
use kube::Api;

/// What the panel knows about the pod it is scoped to.
pub(super) enum PodDetailState {
    Loading,
    Loaded(Box<Pod>),
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
    Found(Box<Pod>),
    NotFound,
}

/// Fetches the pod. A 404 is its own outcome; any other failure is
/// `(message, detail)` - see `PodDetailState::Failed`'s doc comment.
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
    Ok(PodFetch::Found(Box::new(pod)))
}
