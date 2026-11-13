//! Subscribing consumers to a context's shared Helm release watch for one
//! scope (helm-releases D1): the same refcounted start and stop as the Event
//! and kind watches, keyed by `WatchKey::HelmReleases(scope)`, with 401s going
//! through the session's one credential-refresh path.

use super::registry::HelmWatch;
use super::*;

// UNWIRED(#370): the Helm panels (helm-releases sections 3-4) subscribe;
// until then only the session tests do.
#[allow(dead_code)]
impl ClusterRegistry {
    /// Subscribes to `context_name`'s Helm release watch over `scope`, starting
    /// it on the 0-to-1 transition. Returns the shared table to render from.
    pub(crate) fn subscribe_helm_releases(
        cx: &mut App,
        context_name: &str,
        client: Client,
        scope: HelmScope,
    ) -> Entity<HelmReleasesTable> {
        Self::ensure_init(cx, context_name);
        let session = cx
            .global_mut::<Self>()
            .sessions
            .get_mut(context_name)
            .unwrap();
        let table = match session.helm.get(&scope) {
            Some(watch) => watch.table.clone(),
            None => {
                let table = cx.new(|_| HelmReleasesTable::default());
                cx.global_mut::<Self>()
                    .sessions
                    .get_mut(context_name)
                    .unwrap()
                    .helm
                    .insert(
                        scope.clone(),
                        HelmWatch {
                            table: table.clone(),
                            task: None,
                        },
                    );
                table
            }
        };
        let session = cx
            .global_mut::<Self>()
            .sessions
            .get_mut(context_name)
            .unwrap();
        if session
            .watchers
            .subscribe(WatchKey::HelmReleases(scope.clone()))
        {
            session.client = Some(client.clone());
            let task = Self::start_helm_watch(cx, context_name, client, &scope);
            if let Some(watch) = cx
                .global_mut::<Self>()
                .sessions
                .get_mut(context_name)
                .unwrap()
                .helm
                .get_mut(&scope)
            {
                watch.task = Some(task);
            }
        }
        table
    }

    /// Starts `scope`'s Helm release watch on `context_name` against `client`.
    /// Shared by the first subscribe and every restart.
    pub(super) fn start_helm_watch(
        cx: &mut App,
        context_name: &str,
        client: Client,
        scope: &HelmScope,
    ) -> gpui_kit::Task<()> {
        let table = cx.global::<Self>().sessions[context_name].helm[scope]
            .table
            .clone();
        let context_name = context_name.to_string();
        watch_helm_releases(
            client,
            scope,
            table,
            move |cx| Self::handle_unauthorized(cx, &context_name),
            cx,
        )
    }

    /// Unsubscribes from `context_name`'s Helm release watch over `scope`. The
    /// last one out stops the watch and drops its table.
    pub(crate) fn unsubscribe_helm_releases(cx: &mut App, context_name: &str, scope: &HelmScope) {
        if !cx.has_global::<Self>() {
            return;
        }
        let Some(session) = cx.global_mut::<Self>().sessions.get_mut(context_name) else {
            return;
        };
        if session
            .watchers
            .unsubscribe(&WatchKey::HelmReleases(scope.clone()))
        {
            session.helm.remove(scope);
        }
    }

    /// Test-only: whether `context_name` is consuming `scope`'s release watch.
    #[cfg(test)]
    pub(crate) fn helm_watch_running(cx: &App, context_name: &str, scope: &HelmScope) -> bool {
        cx.try_global::<Self>()
            .and_then(|registry| registry.sessions.get(context_name))
            .and_then(|session| session.helm.get(scope))
            .is_some_and(|watch| watch.task.is_some())
    }
}

#[cfg(test)]
mod tests;
