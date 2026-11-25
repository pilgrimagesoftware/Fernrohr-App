//! Subscribing panels to a context's shared watch of one discovered kind (the generic
//! list, `standard-resource-panels` D3): the same refcounted start/stop as the Pods
//! watch, keyed by `WatchKey::Kind`, with 401s going through the session's one
//! credential-refresh path.

use super::kind_scope::{ClusterWide, scope_for};
use super::registry::KindWatch;
use super::*;

fn key_of(key: &KindKey) -> WatchKey {
    WatchKey::Kind(key.clone())
}

impl ClusterRegistry {
    /// Subscribes a panel to `context_name`'s shared cluster-wide watch of `kind`, starting
    /// it on the 0-to-1 transition. Returns the shared table to render from.
    pub fn subscribe_kind(
        cx: &mut App,
        context_name: &str,
        client: Client,
        kind: &DiscoveredKind,
    ) -> Entity<ObjectsTable> {
        Self::subscribe_key(cx, context_name, client, KindKey::all(kind))
    }

    /// Subscribes to the watch that serves `namespace` of `kind` (#446): the cluster-wide one
    /// when a panel already holds it and the server allows it, else one limited to the
    /// namespace (see [`scope_for`]). Release with [`Self::unsubscribe_kind_in`].
    pub(crate) fn subscribe_kind_in(
        cx: &mut App,
        context_name: &str,
        client: Client,
        kind: &DiscoveredKind,
        namespace: Option<&str>,
    ) -> KindSubscription {
        Self::ensure_init(cx, context_name);
        let cluster_wide = Self::cluster_wide_state(cx, context_name, kind);
        let key = KindKey {
            kind: kind.clone(),
            namespace: scope_for(kind.namespaced, namespace, cluster_wide),
        };
        let table = Self::subscribe_key(cx, context_name, client, key.clone());
        KindSubscription { key, table }
    }

    /// Releases a [`Self::subscribe_kind_in`] subscription.
    pub(crate) fn unsubscribe_kind_in(cx: &mut App, context_name: &str, key: &KindKey) {
        Self::unsubscribe_key(cx, context_name, key);
    }

    /// The client `context_name`'s watches were last started with, if any has been.
    pub(super) fn session_client(cx: &App, context_name: &str) -> Option<Client> {
        cx.try_global::<Self>()?
            .sessions
            .get(context_name)?
            .client
            .clone()
    }

    /// How `kind`'s cluster-wide watch on `context_name` stands, for [`scope_for`].
    fn cluster_wide_state(cx: &App, context_name: &str, kind: &DiscoveredKind) -> ClusterWide {
        let Some(session) = cx
            .try_global::<Self>()
            .and_then(|registry| registry.sessions.get(context_name))
        else {
            return ClusterWide::Absent;
        };
        let key = KindKey::all(kind);
        if session.watchers.refcount(&key_of(&key)) == 0 {
            return ClusterWide::Absent;
        }
        match session.kinds.get(&key) {
            Some(watch) if watch.table.read(cx).refused().is_some() => ClusterWide::Refused,
            Some(_) => ClusterWide::Serving,
            None => ClusterWide::Absent,
        }
    }

    fn subscribe_key(
        cx: &mut App,
        context_name: &str,
        client: Client,
        key: KindKey,
    ) -> Entity<ObjectsTable> {
        Self::ensure_init(cx, context_name);
        if !cx.global::<Self>().sessions[context_name]
            .kinds
            .contains_key(&key)
        {
            let table = cx.new(|_| ObjectsTable::for_kind(&key.kind));
            cx.global_mut::<Self>()
                .sessions
                .get_mut(context_name)
                .unwrap()
                .kinds
                .insert(key.clone(), KindWatch { table, task: None });
        }
        let session = cx
            .global_mut::<Self>()
            .sessions
            .get_mut(context_name)
            .unwrap();
        let table = session.kinds[&key].table.clone();
        if session.watchers.subscribe(key_of(&key)) {
            session.client = Some(client.clone());
            let task = Self::start_kind_watch(cx, context_name, client, &key);
            if let Some(watch) = cx
                .global_mut::<Self>()
                .sessions
                .get_mut(context_name)
                .unwrap()
                .kinds
                .get_mut(&key)
            {
                watch.task = Some(task);
            }
        }
        table
    }

    /// Starts `key`'s watch on `context_name` against `client`. Shared by the initial
    /// subscribe and every restart (health resume, a refreshed credential).
    pub(super) fn start_kind_watch(
        cx: &mut App,
        context_name: &str,
        client: Client,
        key: &KindKey,
    ) -> gpui_kit::Task<()> {
        let table = cx.global::<Self>().sessions[context_name].kinds[key]
            .table
            .clone();
        let context_name = context_name.to_string();
        watch_kind(
            client,
            &key.kind,
            key.namespace.as_deref(),
            table,
            move |cx| Self::handle_unauthorized(cx, &context_name),
            cx,
        )
    }

    /// The kinds `context_name` already watches, each with its shared table -
    /// read only, so a caller looking through them starts no new watch.
    pub fn open_kind_tables(
        cx: &App,
        context_name: &str,
    ) -> Vec<(DiscoveredKind, Entity<ObjectsTable>)> {
        cx.try_global::<Self>()
            .and_then(|registry| registry.sessions.get(context_name))
            .map(|session| {
                session
                    .kinds
                    .iter()
                    .map(|(key, watch)| (key.kind.clone(), watch.table.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Unsubscribes a panel from `context_name`'s shared cluster-wide watch of `kind`. The
    /// last one out stops the watch and drops the kind's table.
    pub fn unsubscribe_kind(cx: &mut App, context_name: &str, kind: &DiscoveredKind) {
        Self::unsubscribe_key(cx, context_name, &KindKey::all(kind));
    }

    fn unsubscribe_key(cx: &mut App, context_name: &str, key: &KindKey) {
        if !cx.has_global::<Self>() {
            return;
        }
        let Some(session) = cx.global_mut::<Self>().sessions.get_mut(context_name) else {
            return;
        };
        if session.watchers.unsubscribe(&key_of(key)) {
            session.kinds.remove(key);
        }
    }

    /// `context_name`'s table of `kind` if some panel is watching it - its
    /// objects as already listed. Unlike [`Self::subscribe_kind`], never starts a
    /// watch: for a reader that only uses what is cached (the exec library's
    /// applies-to check and prefill, `exec-command-library` D3, D4).
    #[cfg(test)]
    pub fn existing_kind(
        cx: &App,
        context_name: &str,
        kind: &DiscoveredKind,
    ) -> Option<Entity<ObjectsTable>> {
        cx.try_global::<Self>()
            .and_then(|registry| registry.sessions.get(context_name))
            .and_then(|session| session.kinds.get(&KindKey::all(kind)))
            .map(|watch| watch.table.clone())
    }

    /// [`Self::existing_kind`] for one namespace: the cluster-wide table if some panel holds
    /// it, else the one limited to `namespace`.
    pub fn existing_kind_in(
        cx: &App,
        context_name: &str,
        kind: &DiscoveredKind,
        namespace: &str,
    ) -> Option<Entity<ObjectsTable>> {
        let session = cx.try_global::<Self>()?.sessions.get(context_name)?;
        [None, Some(namespace.to_string())]
            .into_iter()
            .find_map(|namespace| {
                let key = KindKey {
                    kind: kind.clone(),
                    namespace,
                };
                session.kinds.get(&key).map(|watch| watch.table.clone())
            })
    }

    /// Test-only: whether `context_name` is consuming a watch of `kind` right now.
    #[cfg(test)]
    pub(crate) fn kind_watch_running(cx: &App, context_name: &str, kind: &DiscoveredKind) -> bool {
        cx.try_global::<Self>()
            .and_then(|registry| registry.sessions.get(context_name))
            .and_then(|session| session.kinds.get(&KindKey::all(kind)))
            .is_some_and(|watch| watch.task.is_some())
    }
}

#[cfg(test)]
mod tests;
