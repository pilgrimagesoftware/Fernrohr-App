//! Subscribing panels to a context's shared watch of one discovered kind (the generic
//! list, `standard-resource-panels` D3): the same refcounted start/stop as the Pods
//! watch, keyed by `WatchKey::Kind`, with 401s going through the session's one
//! credential-refresh path.

use super::registry::KindWatch;
use super::*;

impl ClusterRegistry {
    /// Subscribes a panel to `context_name`'s shared watch of `kind`, starting it on the
    /// 0-to-1 transition. Returns the shared table to render from.
    pub fn subscribe_kind(
        cx: &mut App,
        context_name: &str,
        client: Client,
        kind: &DiscoveredKind,
    ) -> Entity<ObjectsTable> {
        Self::ensure_init(cx, context_name);
        if !cx.global::<Self>().sessions[context_name]
            .kinds
            .contains_key(kind)
        {
            let table = cx.new(|_| ObjectsTable::default());
            cx.global_mut::<Self>()
                .sessions
                .get_mut(context_name)
                .unwrap()
                .kinds
                .insert(kind.clone(), KindWatch { table, task: None });
        }
        let session = cx
            .global_mut::<Self>()
            .sessions
            .get_mut(context_name)
            .unwrap();
        let table = session.kinds[kind].table.clone();
        if session.watchers.subscribe(WatchKey::Kind(kind.clone())) {
            session.client = Some(client.clone());
            let task = Self::start_kind_watch(cx, context_name, client, kind);
            if let Some(watch) = cx
                .global_mut::<Self>()
                .sessions
                .get_mut(context_name)
                .unwrap()
                .kinds
                .get_mut(kind)
            {
                watch.task = Some(task);
            }
        }
        table
    }

    /// Starts `kind`'s watch on `context_name` against `client`. Shared by the initial
    /// subscribe and every restart (health resume, a refreshed credential).
    pub(super) fn start_kind_watch(
        cx: &mut App,
        context_name: &str,
        client: Client,
        kind: &DiscoveredKind,
    ) -> gpui_kit::Task<()> {
        let table = cx.global::<Self>().sessions[context_name].kinds[kind]
            .table
            .clone();
        let context_name = context_name.to_string();
        watch_kind(
            client,
            kind,
            table,
            move |cx| Self::handle_unauthorized(cx, &context_name),
            cx,
        )
    }

    /// Unsubscribes a panel from `context_name`'s shared watch of `kind`. The last one
    /// out stops the watch and drops the kind's table.
    pub fn unsubscribe_kind(cx: &mut App, context_name: &str, kind: &DiscoveredKind) {
        if !cx.has_global::<Self>() {
            return;
        }
        let Some(session) = cx.global_mut::<Self>().sessions.get_mut(context_name) else {
            return;
        };
        if session.watchers.unsubscribe(&WatchKey::Kind(kind.clone())) {
            session.kinds.remove(kind);
        }
    }

    /// Test-only: whether `context_name` is consuming a watch of `kind` right now.
    #[cfg(test)]
    pub(crate) fn kind_watch_running(cx: &App, context_name: &str, kind: &DiscoveredKind) -> bool {
        cx.try_global::<Self>()
            .and_then(|registry| registry.sessions.get(context_name))
            .and_then(|session| session.kinds.get(kind))
            .is_some_and(|watch| watch.task.is_some())
    }
}

#[cfg(test)]
mod tests;
