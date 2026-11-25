//! The Pods watch that serves a reader needing one namespace's pods (#457): the Controls
//! field's pods, a claim's Used By. The pods [`kind_scope`](super::kind_scope) is for
//! generic kinds; the typed Pods watch has its own table type, so it gets its own
//! one-namespace watches (`WatchKey::PodsIn`) and its own follow.
//!
//! An open all-namespaces Pods watch the server allows serves the reader, so nothing new
//! starts. Otherwise - none open, or refused for an account that may only list pods in
//! some namespaces - the reader watches its namespace alone. A reader that shared the
//! all-namespaces watch moves off it if the server later refuses it.

use super::kind_scope::{ClusterWide, scope_for};
use super::registry::PodsWatch;
use super::*;

impl ClusterRegistry {
    /// Subscribes a reader to `context_name`'s Pods watch of `namespace` alone, starting it
    /// on the 0-to-1 transition. Release with [`Self::unsubscribe_pods_in`].
    fn subscribe_pods_in(
        cx: &mut App,
        context_name: &str,
        client: Client,
        namespace: &str,
    ) -> Entity<PodsTable> {
        Self::ensure_init(cx, context_name);
        if !cx.global::<Self>().sessions[context_name]
            .pods_in
            .contains_key(namespace)
        {
            let table = cx.new(|_| PodsTable::default());
            cx.global_mut::<Self>()
                .sessions
                .get_mut(context_name)
                .unwrap()
                .pods_in
                .insert(namespace.to_string(), PodsWatch { table, task: None });
        }
        let session = cx
            .global_mut::<Self>()
            .sessions
            .get_mut(context_name)
            .unwrap();
        let table = session.pods_in[namespace].table.clone();
        if session
            .watchers
            .subscribe(WatchKey::PodsIn(namespace.to_string()))
        {
            session.client = Some(client.clone());
            let task = Self::start_pods_in_watch(cx, context_name, client, namespace);
            if let Some(watch) = cx
                .global_mut::<Self>()
                .sessions
                .get_mut(context_name)
                .unwrap()
                .pods_in
                .get_mut(namespace)
            {
                watch.task = Some(task);
            }
        }
        table
    }

    /// Starts `namespace`'s Pods watch on `context_name` against `client`. Shared by the
    /// first subscribe and every restart (health resume, a refreshed credential).
    pub(super) fn start_pods_in_watch(
        cx: &mut App,
        context_name: &str,
        client: Client,
        namespace: &str,
    ) -> gpui_kit::Task<()> {
        let table = cx.global::<Self>().sessions[context_name].pods_in[namespace]
            .table
            .clone();
        let context_name = context_name.to_string();
        watch_pods(
            client,
            Some(namespace),
            table,
            move |cx| Self::handle_unauthorized(cx, &context_name),
            cx,
        )
    }

    /// Releases a [`Self::subscribe_pods_in`] subscription. The last one out stops the
    /// watch and drops its table.
    fn unsubscribe_pods_in(cx: &mut App, context_name: &str, namespace: &str) {
        if !cx.has_global::<Self>() {
            return;
        }
        let Some(session) = cx.global_mut::<Self>().sessions.get_mut(context_name) else {
            return;
        };
        if session
            .watchers
            .unsubscribe(&WatchKey::PodsIn(namespace.to_string()))
        {
            session.pods_in.remove(namespace);
        }
    }

    /// How `context_name`'s all-namespaces Pods watch stands, for [`scope_for`].
    fn pods_cluster_wide_state(cx: &App, context_name: &str) -> ClusterWide {
        let Some(session) = cx
            .try_global::<Self>()
            .and_then(|registry| registry.sessions.get(context_name))
        else {
            return ClusterWide::Absent;
        };
        if session.watchers.refcount(&WatchKey::Pods) == 0 {
            ClusterWide::Absent
        } else if session.pods.read(cx).refused().is_some() {
            ClusterWide::Refused
        } else {
            ClusterWide::Serving
        }
    }
}

/// A reader's hold on the Pods watch that serves one namespace: the all-namespaces one
/// while it serves, else the namespace's own.
pub(crate) struct PodsFollow {
    context_name: String,
    /// The namespace the reader needs; `None` for every namespace.
    namespace: Option<String>,
    /// The watch held: `None` for the all-namespaces one.
    held: Option<String>,
    table: Entity<PodsTable>,
}

impl PodsFollow {
    /// Subscribes to the Pods watch that serves `namespace` on `context_name`.
    pub(crate) fn subscribe(
        cx: &mut App,
        context_name: &str,
        client: Client,
        namespace: Option<&str>,
    ) -> Self {
        let cluster_wide = ClusterRegistry::pods_cluster_wide_state(cx, context_name);
        let held = scope_for(true, namespace, cluster_wide);
        let table = Self::hold(cx, context_name, client, held.as_deref());
        Self {
            context_name: context_name.to_string(),
            namespace: namespace.map(str::to_string),
            held,
            table,
        }
    }

    fn hold(
        cx: &mut App,
        context_name: &str,
        client: Client,
        held: Option<&str>,
    ) -> Entity<PodsTable> {
        match held {
            Some(namespace) => {
                ClusterRegistry::subscribe_pods_in(cx, context_name, client, namespace)
            }
            None => ClusterRegistry::subscribe_pods(cx, context_name, client),
        }
    }

    /// The table to read now. It changes when the follow moves.
    pub(crate) fn table(&self) -> &Entity<PodsTable> {
        &self.table
    }

    /// Moves to the reader's namespace's own watch if the all-namespaces one it shares has
    /// been refused, releasing that one. Returns whether it moved. A reader of every
    /// namespace has nothing narrower to move to.
    pub(crate) fn reroute_if_refused(&mut self, cx: &mut App) -> bool {
        let Some(namespace) = self.namespace.clone() else {
            return false;
        };
        if self.held.is_some() || self.table.read(cx).refused().is_none() {
            return false;
        }
        let Some(client) = ClusterRegistry::session_client(cx, &self.context_name) else {
            return false;
        };
        let table = ClusterRegistry::subscribe_pods_in(cx, &self.context_name, client, &namespace);
        self.release(cx);
        self.held = Some(namespace);
        self.table = table;
        true
    }

    /// Releases the hold, as the reader closing does.
    pub(crate) fn release(&self, cx: &mut App) {
        match &self.held {
            Some(namespace) => {
                ClusterRegistry::unsubscribe_pods_in(cx, &self.context_name, namespace)
            }
            None => ClusterRegistry::unsubscribe_pods(cx, &self.context_name),
        }
    }

    /// A follow of `table` that belongs to no session, for a test that fills the table
    /// itself: it never moves, and releasing it releases nothing.
    #[cfg(test)]
    pub(crate) fn detached(table: Entity<PodsTable>) -> Self {
        Self {
            context_name: String::new(),
            namespace: None,
            held: None,
            table,
        }
    }
}

#[cfg(test)]
#[path = "pods_scope_tests.rs"]
mod tests;
